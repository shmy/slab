use std::fmt;

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use rootcause::Report;

#[derive(Debug, Clone)]
pub enum WebError {
    /// JSON 请求体语法错误、类型不匹配、或 DTO 校验失败（含自定义 Deserialize）。
    /// `key` 为 l10n key（serde 错误由 [`crate::extract::valid_json`] 分类映射，validify 错误由 `errors_to_key` 产出），
    /// `field` 为出错字段的完整路径（serde_path_to_error 提供，可 None）。
    InvalidRequestBody { key: String, field: Option<String> },
    /// Path 参数无法反序列化到目标类型（如 `{id}` 与路径参数类型不匹配）。
    /// `key` 为 l10n key，`field` 为出错的路径参数名（可 None）。
    InvalidPathParams { key: String, field: Option<String> },
    /// Query 字符串无法反序列化到目标类型。
    /// `key` 为 l10n key，`field` 为出错的查询字段路径（可 None）。
    InvalidQueryParams { key: String, field: Option<String> },
    /// 本地化 KEY
    L10n(String),
}

impl fmt::Display for WebError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WebError::InvalidRequestBody { key, .. }
            | WebError::InvalidPathParams { key, .. }
            | WebError::InvalidQueryParams { key, .. } => write!(f, "{key}"),
            WebError::L10n(s) => write!(f, "{s}"),
        }
    }
}

impl std::error::Error for WebError {}

impl WebError {
    pub fn status_code(&self) -> StatusCode {
        match self {
            WebError::InvalidRequestBody { .. }
            | WebError::InvalidPathParams { .. }
            | WebError::InvalidQueryParams { .. } => StatusCode::BAD_REQUEST,
            WebError::L10n(key) => l10n_status_code(key),
        }
    }

    pub fn problem_type(&self) -> &'static str {
        match self {
            WebError::InvalidRequestBody { .. } => "urn:slab:problem:invalid-request-body",
            WebError::InvalidPathParams { .. } => "urn:slab:problem:invalid-path-params",
            WebError::InvalidQueryParams { .. } => "urn:slab:problem:invalid-query-params",
            WebError::L10n(key) => match l10n_status_code(key) {
                StatusCode::UNAUTHORIZED => "urn:slab:problem:unauthorized",
                StatusCode::INTERNAL_SERVER_ERROR => "urn:slab:problem:internal-server-error",
                _ => "urn:slab:problem:domain-error",
            },
        }
    }

    pub fn problem_title(&self) -> &'static str {
        match self {
            WebError::InvalidRequestBody { .. } => "Invalid request body",
            WebError::InvalidPathParams { .. } => "Invalid path parameters",
            WebError::InvalidQueryParams { .. } => "Invalid query parameters",
            WebError::L10n(key) => match l10n_status_code(key) {
                StatusCode::UNAUTHORIZED => "Unauthorized",
                StatusCode::INTERNAL_SERVER_ERROR => "Internal server error",
                _ => "Domain error",
            },
        }
    }

    pub fn error_code(&self) -> &str {
        match self {
            WebError::InvalidRequestBody { .. } => "invalid_request_body",
            WebError::InvalidPathParams { .. } => "invalid_path_params",
            WebError::InvalidQueryParams { .. } => "invalid_query_params",
            WebError::L10n(key) => key.as_str(),
        }
    }
}

fn l10n_status_code(key: &str) -> StatusCode {
    if key.starts_with("access_token_") || key == "authed_account_not_found" {
        return StatusCode::UNAUTHORIZED;
    }
    if key == "internal_server_error" {
        return StatusCode::INTERNAL_SERVER_ERROR;
    }
    if key.ends_with("_version_conflict") {
        return StatusCode::CONFLICT;
    }
    StatusCode::BAD_REQUEST
}

impl IntoResponse for WebError {
    fn into_response(self) -> Response {
        let mut response = Response::default();
        response.extensions_mut().insert(self);
        response
    }
}

impl From<Report> for WebError {
    fn from(report: Report) -> Self {
        tracing::error!(error = %report, "Request failed");
        report_to_web_error(report)
    }
}

fn report_to_web_error(report: Report) -> WebError {
    let msg = report.format_current_context().to_string();
    if msg
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
    {
        WebError::L10n(msg)
    } else {
        WebError::L10n("internal_server_error".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct FakeL10nError;

    impl fmt::Display for FakeL10nError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "account_not_found")
        }
    }

    impl std::error::Error for FakeL10nError {}

    #[derive(Debug)]
    struct FakeInternalError;

    impl fmt::Display for FakeInternalError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "connection refused")
        }
    }

    impl std::error::Error for FakeInternalError {}

    #[test]
    fn test_report_with_l10n_key() {
        let report = Report::from(FakeL10nError);
        let web_err = report_to_web_error(report);
        assert!(matches!(web_err, WebError::L10n(key) if key == "account_not_found"));
    }

    #[test]
    fn test_report_without_l10n_prefix() {
        let report = Report::from(FakeInternalError);
        let web_err = report_to_web_error(report);
        assert!(matches!(web_err, WebError::L10n(key) if key == "internal_server_error"));
    }

    #[test]
    fn test_internal_server_error_maps_to_500_problem() {
        let err = WebError::L10n("internal_server_error".to_string());
        assert_eq!(err.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(err.problem_type(), "urn:slab:problem:internal-server-error");
        assert_eq!(err.problem_title(), "Internal server error");
    }

    #[test]
    fn test_domain_l10n_error_maps_to_400_problem() {
        let err = WebError::L10n("account_not_found".to_string());
        assert_eq!(err.status_code(), StatusCode::BAD_REQUEST);
        assert_eq!(err.problem_type(), "urn:slab:problem:domain-error");
        assert_eq!(err.problem_title(), "Domain error");
    }

    #[test]
    fn test_auth_l10n_errors_map_to_401_problem() {
        let err = WebError::L10n("access_token_invalid".to_string());
        assert_eq!(err.status_code(), StatusCode::UNAUTHORIZED);
        assert_eq!(err.problem_type(), "urn:slab:problem:unauthorized");
        assert_eq!(err.problem_title(), "Unauthorized");
    }
}

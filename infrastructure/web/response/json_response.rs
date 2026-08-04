use axum::{
    Json,
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::{Serialize, Serializer};
use shared_contract::query::paging_result::PagingResult;
use utoipa::ToSchema;

use crate::error::WebError;

pub type JsonResponseType<T> = Result<JsonResponse<T>, WebError>;
pub type JsonResponsePagingType<T> = Result<JsonResponse<PagingResult<T>>, WebError>;
/// 仅用于 utoipa `responses` 宏中声明空 body 的 schema 类型（`body = JsonResponseEmpty`）。
/// **不是** handler 的返回类型，handler 应使用 [`JsonResponseType<()>`]。
pub type JsonResponseEmpty = JsonResponse<()>;

#[derive(Debug, Serialize, ToSchema)]
#[serde(untagged)]
pub enum JsonResponse<T = ()>
where
    T: Serialize + ToSchema,
{
    Ok(T),
}

impl<T> JsonResponse<T>
where
    T: Serialize + ToSchema,
{
    pub fn ok(data: T) -> Result<Self, WebError> {
        Ok(JsonResponse::Ok(data))
    }
}

impl<T> IntoResponse for JsonResponse<T>
where
    T: Serialize + ToSchema,
{
    fn into_response(self) -> Response {
        match self {
            JsonResponse::Ok(data) => Json(data).into_response(),
        }
    }
}

/// RFC 9457 Problem Details (`application/problem+json`)
#[derive(Debug, Serialize, ToSchema)]
pub struct ProblemDetails {
    #[serde(rename = "type")]
    pub type_url: String,
    pub title: String,
    #[serde(serialize_with = "serialize_status_code")]
    #[schema(value_type = u16, example = 400)]
    pub status: StatusCode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instance: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
}

fn serialize_status_code<S>(status: &StatusCode, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_u16(status.as_u16())
}

impl ProblemDetails {
    pub fn new(
        type_url: impl Into<String>,
        title: impl Into<String>,
        status: StatusCode,
        detail: Option<String>,
        instance: Option<String>,
        error_code: Option<String>,
        trace_id: Option<String>,
    ) -> Self {
        Self {
            type_url: type_url.into(),
            title: title.into(),
            status,
            detail,
            instance,
            error_code,
            trace_id,
        }
    }
}

impl IntoResponse for ProblemDetails {
    fn into_response(self) -> Response {
        let mut response = (self.status, Json(self)).into_response();
        response.headers_mut().insert(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static("application/problem+json"),
        );
        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;

    #[tokio::test]
    async fn test_problem_details_content_type_is_problem_json() {
        let response = ProblemDetails::new(
            "urn:slab:problem:test",
            "Test",
            StatusCode::BAD_REQUEST,
            Some("detail".to_string()),
            Some("/test".to_string()),
            Some("test_error".to_string()),
            Some("trace-1".to_string()),
        )
        .into_response();

        let content_type = response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        assert_eq!(content_type, "application/problem+json");
    }

    #[tokio::test]
    async fn test_problem_details_status_matches_http_status() {
        let response = ProblemDetails::new(
            "urn:slab:problem:test",
            "Test",
            StatusCode::UNPROCESSABLE_ENTITY,
            Some("detail".to_string()),
            Some("/test".to_string()),
            Some("test_error".to_string()),
            Some("trace-2".to_string()),
        )
        .into_response();

        let http_status = response.status().as_u16();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"].as_u64().unwrap() as u16, http_status);
    }

    #[tokio::test]
    async fn test_problem_details_body_contains_instance_path() {
        let response = ProblemDetails::new(
            "urn:slab:problem:test",
            "Test",
            StatusCode::BAD_REQUEST,
            Some("detail".to_string()),
            Some("/api/v1/auth/refresh".to_string()),
            Some("test_error".to_string()),
            Some("trace-3".to_string()),
        )
        .into_response();

        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["instance"], "/api/v1/auth/refresh");
    }
}

use std::ops::Deref;

use axum::extract::path::ErrorKind;
use axum::extract::rejection::PathRejection;
use axum::extract::{FromRequestParts, Path};
use axum::http::request::Parts;
use serde::de::DeserializeOwned;
use validify::Validify;

use crate::error::{InvalidParamsKind, WebError};

/// Path 参数提取器：反序列化后执行 **validify** 校验。
/// 反序列化失败 → [`WebError::InvalidParams`]（kind = Path，l10n key + 字段路径）；校验失败 → 同上（l10n key）。
/// 继续用 Axum `Path`（路径参数匹配/解码在其内部），错误经结构化 `ErrorKind` 分类。
#[derive(Debug)]
pub struct ValidPath<T>(pub T);

impl<T> Deref for ValidPath<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T, S> FromRequestParts<S> for ValidPath<T>
where
    T: DeserializeOwned + Validify + Send,
    S: Send + Sync,
{
    type Rejection = WebError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let Path(mut inner) = Path::<T>::from_request_parts(parts, state)
            .await
            .map_err(path_rejection_to_web)?;
        inner.validify().map_err(|e| WebError::InvalidParams {
            kind: InvalidParamsKind::Path,
            key: super::validify_util::errors_to_key(e),
            field: None,
        })?;
        Ok(ValidPath(inner))
    }
}

fn path_rejection_to_web(r: PathRejection) -> WebError {
    match r {
        PathRejection::FailedToDeserializePathParams(e) => {
            let (key, field) = error_kind_to_l10n(e.kind());
            tracing::debug!(path_err = %e.kind(), "path params rejected");
            WebError::InvalidParams {
                kind: InvalidParamsKind::Path,
                key: key.to_string(),
                field,
            }
        }
        _ => {
            tracing::debug!("path params rejected: missing path params");
            WebError::InvalidParams {
                kind: InvalidParamsKind::Path,
                key: crate::l10n_keys::INVALID_PATH_PARAMS.to_string(),
                field: None,
            }
        }
    }
}

/// axum `ErrorKind`（结构化）→ (l10n key, 字段路径)。
fn error_kind_to_l10n(kind: &ErrorKind) -> (&'static str, Option<String>) {
    match kind {
        ErrorKind::ParseErrorAtKey { key, .. } => (
            crate::l10n_keys::PATH_PARAMS_INVALID_TYPE,
            Some(key.clone()),
        ),
        ErrorKind::ParseErrorAtIndex { .. } | ErrorKind::ParseError { .. } => {
            (crate::l10n_keys::PATH_PARAMS_INVALID_TYPE, None)
        }
        ErrorKind::InvalidUtf8InPathParam { key } => (
            crate::l10n_keys::PATH_PARAMS_INVALID_TYPE,
            Some(key.clone()),
        ),
        ErrorKind::DeserializeError { key, .. } => {
            (crate::l10n_keys::PATH_PARAMS_PARSE_ERROR, Some(key.clone()))
        }
        ErrorKind::WrongNumberOfParameters { .. } => {
            (crate::l10n_keys::PATH_PARAMS_WRONG_COUNT, None)
        }
        _ => (crate::l10n_keys::INVALID_PATH_PARAMS, None),
    }
}

#[cfg(test)]
mod tests {
    use super::error_kind_to_l10n;
    use axum::extract::path::ErrorKind;

    #[test]
    fn parse_error_at_key_has_field() {
        let kind = ErrorKind::ParseErrorAtKey {
            key: "id".into(),
            value: "abc".into(),
            expected_type: "u64",
        };
        let (key, field) = error_kind_to_l10n(&kind);
        assert_eq!(key, crate::l10n_keys::PATH_PARAMS_INVALID_TYPE);
        assert_eq!(field.as_deref(), Some("id"));
    }

    #[test]
    fn deserialize_error_has_field() {
        let kind = ErrorKind::DeserializeError {
            key: "code".into(),
            value: "x".into(),
            message: "bad".into(),
        };
        let (key, field) = error_kind_to_l10n(&kind);
        assert_eq!(key, crate::l10n_keys::PATH_PARAMS_PARSE_ERROR);
        assert_eq!(field.as_deref(), Some("code"));
    }

    #[test]
    fn parse_error_without_key_has_no_field() {
        let kind = ErrorKind::ParseError {
            value: "abc".into(),
            expected_type: "u64",
        };
        let (key, field) = error_kind_to_l10n(&kind);
        assert_eq!(key, crate::l10n_keys::PATH_PARAMS_INVALID_TYPE);
        assert_eq!(field, None);
    }

    #[test]
    fn wrong_number_of_params_has_no_field() {
        let kind = ErrorKind::WrongNumberOfParameters {
            got: 1,
            expected: 2,
        };
        let (key, field) = error_kind_to_l10n(&kind);
        assert_eq!(key, crate::l10n_keys::PATH_PARAMS_WRONG_COUNT);
        assert_eq!(field, None);
    }
}

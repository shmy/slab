use std::ops::Deref;

use axum::extract::{FromRequestParts, Query};
use axum::http::request::Parts;
use serde::de::DeserializeOwned;
use validify::Validify;

use crate::error::WebError;

/// Query 提取器：反序列化后执行 **validify** 校验。
/// 反序列化失败 → [`WebError::InvalidQueryParams`]；校验失败 → [`WebError::InvalidQueryParams`]（携带 l10n key）。
#[derive(Debug, Clone)]
pub struct ValidQuery<T>(pub T);

impl<T> Deref for ValidQuery<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T, S> FromRequestParts<S> for ValidQuery<T>
where
    T: DeserializeOwned + Validify + Send,
    S: Send + Sync,
{
    type Rejection = WebError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let Query(mut inner) = Query::<T>::from_request_parts(parts, state)
            .await
            .map_err(|r| WebError::InvalidQueryParams(r.body_text()))?;
        inner
            .validify()
            .map_err(|e| WebError::InvalidQueryParams(super::validify_util::errors_to_key(e)))?;
        Ok(ValidQuery(inner))
    }
}

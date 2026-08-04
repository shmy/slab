use std::ops::Deref;

use axum::extract::{FromRequestParts, Path};
use axum::http::request::Parts;
use serde::de::DeserializeOwned;
use validify::Validify;

use crate::error::WebError;

/// Path 参数提取器：反序列化后执行 **validify** 校验。
/// 反序列化失败 → [`WebError::InvalidPathParams`]；校验失败 → [`WebError::InvalidPathParams`]（携带 l10n key）。
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
            .map_err(|r| WebError::InvalidPathParams(r.body_text()))?;
        inner
            .validify()
            .map_err(|e| WebError::InvalidPathParams(super::validify_util::errors_to_key(e)))?;
        Ok(ValidPath(inner))
    }
}

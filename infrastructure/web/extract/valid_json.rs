use std::ops::Deref;

use axum::Json;
use axum::extract::{FromRequest, Request};
use serde::de::DeserializeOwned;
use validify::Validify;

use crate::error::WebError;

/// JSON 请求体：先按 Axum [`Json`] 反序列化，再执行 **validify**（`#[modify]` + `#[validate]`）。
/// 校验失败时返回 [`WebError::InvalidRequestBody`]，`msg` 为 l10n key。
#[derive(Debug, Clone, Copy, Default)]
pub struct ValidJson<T: Validify>(pub T);

impl<T: Validify> Deref for ValidJson<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T: Validify, S> FromRequest<S> for ValidJson<T>
where
    T: DeserializeOwned + Send,
    S: Send + Sync,
{
    type Rejection = WebError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let Json(mut inner) = Json::<T>::from_request(req, state)
            .await
            .map_err(|r| WebError::InvalidRequestBody(r.body_text()))?;
        inner
            .validify()
            .map_err(|e| WebError::InvalidRequestBody(super::validify_util::errors_to_key(e)))?;
        Ok(ValidJson(inner))
    }
}

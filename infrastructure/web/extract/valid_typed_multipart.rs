use std::ops::Deref;

use axum::extract::{FromRequest, Request};
use axum_typed_multipart::{TryFromMultipartWithState, TypedMultipart};

use crate::error::WebError;

/// `multipart/form-data`：与 [`axum_typed_multipart::TypedMultipart`] 相同解析流程，
/// 失败时映射为 [`WebError::InvalidRequestBody`]，以便走统一 `JsonResponse` / l10n 路径。
#[derive(Debug)]
pub struct ValidTypedMultipart<T>(pub T);

impl<T> Deref for ValidTypedMultipart<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T, S> FromRequest<S> for ValidTypedMultipart<T>
where
    T: TryFromMultipartWithState<S> + Send,
    S: Send + Sync,
{
    type Rejection = WebError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let TypedMultipart(inner) = TypedMultipart::<T>::from_request(req, state)
            .await
            .map_err(|e| WebError::InvalidRequestBody(e.to_string()))?;
        Ok(Self(inner))
    }
}

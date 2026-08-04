use axum::{extract::FromRequestParts, http::request::Parts};

use web::error::WebError;

use shared_contract::value_object::id::ID;

#[derive(Clone, Debug)]
pub struct AuthedAccount(pub ID);

impl<S> FromRequestParts<S> for AuthedAccount
where
    S: Send + Sync + 'static,
{
    type Rejection = WebError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let Some(subject) = parts.extensions.get::<Self>() else {
            return Err(WebError::L10n("authed_account_not_found".to_string()));
        };
        Ok(subject.clone())
    }
}

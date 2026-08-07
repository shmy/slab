use crate::extract::authed_account::AuthedAccount;
use authn_kit::{AuthnError, VerifiedToken, Verifier, access_token_from_parts};
use axum::{
    extract::{Request, State},
    middleware::Next,
    response::{IntoResponse, Response},
};
use kv::KvBackend;
use jwt::{TokenBundle, TokenHelper, TokenRealm};
use shared_contract::value_object::id::ID;
use web::error::WebError;

struct AuthVerifier<'a> {
    token_helper: &'a TokenHelper,
    kv: &'a KvBackend,
}

impl Verifier for AuthVerifier<'_> {
    async fn verify<'a>(&'a self, token: &'a str) -> Result<VerifiedToken, AuthnError> {
        let claims = self
            .token_helper
            .decode_access_token(token)
            .map_err(|_| AuthnError::AccessTokenInvalid)?;

        let realm = self.token_helper.realm();
        let stored_jti = self
            .kv
            .get::<String>(&authn_kit::access_jti_key(realm, &claims.sub))
            .await
            .map_err(|_| AuthnError::AccessTokenInvalid)?;

        match stored_jti {
            Some(ref jti) if jti == &claims.jti => Ok(VerifiedToken {
                subject: claims.sub,
                jti: claims.jti,
            }),
            _ => Err(AuthnError::AccessTokenRevoked),
        }
    }
}

async fn run_auth(
    realm: TokenRealm,
    token_bundle: &TokenBundle,
    kv: &KvBackend,
    request: Request,
    next: Next,
) -> Response {
    let token_helper = match realm {
        TokenRealm::Customer => token_bundle.customer(),
        TokenRealm::Account => token_bundle.account(),
    };
    let verifier = AuthVerifier { token_helper, kv };
    match authenticate(&verifier, request, next).await {
        Ok(response) => response,
        Err(err) => err.into_response(),
    }
}

pub async fn customer_auth_middleware(
    State(token_bundle): State<TokenBundle>,
    State(kv): State<KvBackend>,
    request: Request,
    next: Next,
) -> Response {
    run_auth(TokenRealm::Customer, &token_bundle, &kv, request, next).await
}

pub async fn account_auth_middleware(
    State(token_bundle): State<TokenBundle>,
    State(kv): State<KvBackend>,
    request: Request,
    next: Next,
) -> Response {
    run_auth(TokenRealm::Account, &token_bundle, &kv, request, next).await
}

async fn authenticate(
    verifier: &impl Verifier,
    mut request: Request,
    next: Next,
) -> Result<Response, WebError> {
    let token = access_token_from_parts(request.headers(), request.uri())
        .map_err(auth_error_to_web_error)?;
    let verified = verifier
        .verify(&token)
        .await
        .map_err(auth_error_to_web_error)?;
    let account_id: i64 = verified
        .subject
        .parse()
        .map_err(|_| auth_error_to_web_error(AuthnError::AccessTokenInvalid))?;
    let account_id = ID::new_unchecked(account_id);

    request.extensions_mut().insert(AuthedAccount(account_id));
    Ok(next.run(request).await)
}

fn auth_error_to_web_error(err: AuthnError) -> WebError {
    WebError::L10n(err.to_string())
}

use crate::shared::token_ops;
use appctx::{Backend, TokenBundle, TokenHelper};
use axum::extract::State;
use http_auth::extract::authed_account::AuthedAccount;
use rootcause::Result;
use serde::Serialize;
use shared_contract::value_object::id::ID;
use utoipa::ToSchema;
use web::response::json_response::{JsonResponse, JsonResponseType};

#[derive(Debug, Serialize, ToSchema)]
pub struct LogoutResponse {
    #[schema(value_type = bool, example = true)]
    pub logged_out: bool,
}

#[utoipa::path(
    post,
    path = "/api/v1/identity/logout",
    operation_id = "identity_logout",
    tag = "identity",
    responses((status = 200, body = JsonResponse<LogoutResponse>)),
    security(("bearerAuth" = []))
)]
#[tracing::instrument(skip(kv))]
pub(crate) async fn handler(
    AuthedAccount(account_id): AuthedAccount,
    State(kv): State<Backend>,
    State(token_bundle): State<TokenBundle>,
) -> JsonResponseType<LogoutResponse> {
    let response = execute(&kv, token_bundle.account(), account_id).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip(kv))]
#[inline]
async fn execute(
    kv: &Backend,
    token_helper: &TokenHelper,
    account_id: ID,
) -> Result<LogoutResponse> {
    token_ops::revoke_tokens(kv, token_helper, account_id).await?;
    Ok(LogoutResponse { logged_out: true })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests;
    use appctx::testing;
    use authn_kit::{access_jti_key, refresh_key, subject_refresh_key};
    use migration::run_migrations;

    const REALM: &str = "account";

    #[sqlx::test]
    async fn test_logout_clears_cache_keys(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool).await;
        let id = tests::insert_test_account(&state.pg_pool, "13900001501").await;
        let tokens = token_ops::issue_tokens(&state.kv, state.token_bundle.account(), &id)
            .await
            .unwrap();

        let response = execute(&state.kv, state.token_bundle.account(), id)
            .await
            .unwrap();
        assert!(response.logged_out);

        let refresh_still = state
            .kv
            .get::<String>(&refresh_key(REALM, &tokens.refresh_token))
            .await
            .unwrap();
        assert!(refresh_still.is_none());

        let subject_rev = state
            .kv
            .get::<String>(&subject_refresh_key(REALM, &id))
            .await
            .unwrap();
        assert!(subject_rev.is_none());

        let jti = state
            .kv
            .get::<String>(&access_jti_key(REALM, &id))
            .await
            .unwrap();
        assert!(jti.is_none());
    }
}

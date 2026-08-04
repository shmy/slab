use crate::endpoint::account_login::LoginResponse;
use crate::shared::token_ops;
use appctx::{PgPool, TokenBundle, TokenHelper};
use axum::extract::State;
use identity_contract::value_object::refresh_token::RefreshToken;
use serde::Deserialize;
use utoipa::ToSchema;
use validify::Validify;
use web::extract::valid_json::ValidJson;
use web::response::json_response::{JsonResponse, JsonResponseType};

#[derive(Debug, Deserialize, Validify, ToSchema)]
pub struct RefreshRequest {
    #[schema(example = "1234567890123456789")]
    pub refresh_token: RefreshToken,
}

#[utoipa::path(
    post,
    path = "/api/v1/identity/refresh",
    operation_id = "identity_refresh",
    tag = "identity",
    request_body = RefreshRequest,
    responses((status = 200, body = JsonResponse<LoginResponse>))
)]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    State(token_bundle): State<TokenBundle>,
    ValidJson(request): ValidJson<RefreshRequest>,
) -> JsonResponseType<LoginResponse> {
    let response = execute(&pg_pool, token_bundle.account(), request).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip(pg_pool))]
#[inline]
async fn execute(
    pg_pool: &PgPool,
    token_helper: &TokenHelper,
    request: RefreshRequest,
) -> rootcause::Result<LoginResponse> {
    let account_id =
        token_ops::consume_refresh_token(pg_pool, token_helper, &request.refresh_token).await?;
    let tokens = token_ops::issue_tokens(pg_pool, token_helper, &account_id).await?;

    Ok(LoginResponse::bearer(
        tokens.access_token,
        tokens.refresh_token,
        tokens.expires_in,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests;
    use appctx::testing;
    use migration::run_migrations;
    use shared_contract::value_object::id::ID;

    #[sqlx::test]
    async fn test_refresh_rotates_and_old_refresh_rejected(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool).await;
        let uid = i64::from(tests::insert_test_account(&state.pg_pool, "13900001601").await);
        let first = token_ops::issue_tokens(
            &state.pg_pool,
            state.token_bundle.account(),
            &ID::new_unchecked(uid),
        )
        .await
        .unwrap();
        let rt1 = first.refresh_token.clone();

        let second = execute(
            &state.pg_pool,
            state.token_bundle.account(),
            RefreshRequest {
                refresh_token: RefreshToken::new(rt1.to_string()),
            },
        )
        .await
        .unwrap();
        assert_ne!(second.refresh_token, rt1);

        let err = execute(
            &state.pg_pool,
            state.token_bundle.account(),
            RefreshRequest {
                refresh_token: RefreshToken::new(rt1),
            },
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("refresh_token_invalid"));
    }
}

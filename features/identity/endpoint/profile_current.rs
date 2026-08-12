use axum::extract::State;
use db::PgPool;
use http_auth::extract::authed_account::AuthedAccount;
use identity_contract::port::AccountPort;
use shared_contract::value_object::id::ID;
use web::response::json_response::{JsonResponse, JsonResponseType};

use crate::endpoint::account_get::GetAccountResponse;

/// 当前登录账号自省：从 Bearer 令牌取账号 id，无需客户端解码 JWT。
#[utoipa::path(
    get,
    path = "/api/v1/profile/current",
    operation_id = "profile_current",
    tag = "identity",
    responses((status = 200, body = JsonResponse<GetAccountResponse>)),
    security(("bearerAuth" = []))
)]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    AuthedAccount(account_id): AuthedAccount,
    State(pg_pool): State<PgPool>,
) -> JsonResponseType<GetAccountResponse> {
    let response = execute(&pg_pool, account_id).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip(pg_pool))]
#[inline]
async fn execute(pg_pool: &PgPool, account_id: ID) -> rootcause::Result<GetAccountResponse> {
    let mut conn = pg_pool.acquire().await?;
    let account = AccountPort::by_id(&mut conn, &account_id).await?;
    Ok(GetAccountResponse {
        id: account.id,
        name: account.name,
        phone: account.phone,
        privileged: account.privileged,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests;
    use appctx::testing;
    use migration::run_migrations;

    #[sqlx::test]
    async fn test_current_success(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool).await;
        let account_id = tests::insert_test_account(&state.pg_pool, "13900002001").await;
        let response = execute(&state.pg_pool, account_id).await.unwrap();
        assert_eq!(response.id, account_id);
        assert!(response.name.starts_with("test-"));
        assert_eq!(&*response.phone, "13900002001");
        assert!(!response.privileged);
    }
}

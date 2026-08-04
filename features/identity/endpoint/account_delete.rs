use crate::repository::account_repository::AccountRepository;
use axum::extract::State;
use db::PgPool;
use web::extract::valid_path::ValidPath;
use web::response::json_response::{JsonResponse, JsonResponseType};

use serde::{Deserialize, Serialize};
use shared_contract::value_object::id::ID;
use utoipa::{IntoParams, ToSchema};
use validify::Validify;

#[derive(Debug, Deserialize, Validify, IntoParams)]
#[into_params(parameter_in = Path)]
pub(crate) struct DeleteAccountPath {
    pub id: ID,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct DeleteAccountResponse {
    pub id: ID,
    #[schema(value_type = bool, example = true)]
    pub deleted: bool,
}

#[utoipa::path(
    delete,
    path = "/api/v1/accounts/{id}",
    operation_id = "account_delete",
    tag = "account",
    params(DeleteAccountPath),
    responses((status = 200, body = JsonResponse<DeleteAccountResponse>)),
    security(("bearerAuth" = []))
)]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ValidPath(path): ValidPath<DeleteAccountPath>,
) -> JsonResponseType<DeleteAccountResponse> {
    let response = execute(&pg_pool, path).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip(pg_pool))]
#[inline]
async fn execute(
    pg_pool: &PgPool,
    path: DeleteAccountPath,
) -> rootcause::Result<DeleteAccountResponse> {
    let mut conn = pg_pool.acquire().await?;
    AccountRepository::delete(&mut conn, &path.id).await?;
    Ok(DeleteAccountResponse {
        id: path.id,
        deleted: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests;
    use appctx::testing;
    use identity_contract::port::AccountPort;
    use migration::run_migrations;

    #[sqlx::test]
    async fn test_delete_success(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool).await;
        let account_id = tests::insert_test_account(&state.pg_pool, "13900001801").await;
        let response = execute(&state.pg_pool, DeleteAccountPath { id: account_id })
            .await
            .unwrap();
        assert_eq!(response.id, account_id);
        assert!(response.deleted);

        // 确认真的删了
        let mut conn = state.pg_pool.acquire().await.unwrap();
        let err = AccountPort::by_id(&mut conn, &account_id)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("account_not_found"));
    }

    #[sqlx::test]
    async fn test_delete_idempotent(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool).await;
        let account_id = tests::insert_test_account(&state.pg_pool, "13900001802").await;
        // 第一次删除成功
        execute(&state.pg_pool, DeleteAccountPath { id: account_id })
            .await
            .unwrap();
        // 第二次也成功（幂等）
        let response = execute(&state.pg_pool, DeleteAccountPath { id: account_id })
            .await
            .unwrap();
        assert_eq!(response.id, account_id);
        assert!(response.deleted);
    }

    #[sqlx::test]
    async fn test_delete_nonexistent_ok(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool).await;
        // 删除不存在的 ID 也返回成功（幂等语义）
        let response = execute(
            &state.pg_pool,
            DeleteAccountPath {
                id: ID::from(999_i64),
            },
        )
        .await
        .unwrap();
        assert_eq!(response.id, ID::from(999_i64));
        assert!(response.deleted);
    }
}

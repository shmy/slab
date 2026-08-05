use crate::repository::account_repository::AccountRepository;
use audit_contract::AuditService;
use axum::extract::State;
use db::PgPool;
use http_auth::extract::operator::OperatorContext;
use identity_contract::port::AccountPort;
use sqlx::Acquire;
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
    ctx: OperatorContext,
    ValidPath(path): ValidPath<DeleteAccountPath>,
) -> JsonResponseType<DeleteAccountResponse> {
    let response = execute(&pg_pool, ctx, path).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip(pg_pool))]
#[inline]
async fn execute(
    pg_pool: &PgPool,
    ctx: OperatorContext,
    path: DeleteAccountPath,
) -> rootcause::Result<DeleteAccountResponse> {
    let mut conn = pg_pool.acquire().await?;
    let mut txn = conn.begin().await?;

    // 删除前读旧值用于变更历史；账户不存在时幂等删除，不产生记录
    let before = match AccountPort::by_id(&mut txn, &path.id).await {
        Ok(account) => Some(account),
        Err(err) if err.to_string().contains("account_not_found") => None,
        Err(err) => return Err(err),
    };
    AccountRepository::delete(&mut txn, &path.id).await?;
    if let Some(before) = before {
        AuditService::record_deleted(&mut txn, "account", &path.id, &ctx, &before).await?;
    }
    txn.commit().await?;

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
        let response = execute(
            &state.pg_pool,
            tests::test_operator_context(),
            DeleteAccountPath { id: account_id },
        )
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

        // 变更历史：delete 类型，after 为空
        let audit_row = sqlx::query!(
            r#"SELECT action, before, after FROM audit_logs WHERE entity_id = $1"#,
            *account_id
        )
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        assert_eq!(audit_row.action, 3); // Deleted
        let before: serde_json::Value = audit_row.before.unwrap();
        assert_eq!(before["name"], "test-13900001801");
        assert!(before.get("password").is_none());
        assert!(audit_row.after.is_none());
    }

    #[sqlx::test]
    async fn test_delete_idempotent(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool).await;
        let account_id = tests::insert_test_account(&state.pg_pool, "13900001802").await;
        // 第一次删除成功
        execute(
            &state.pg_pool,
            tests::test_operator_context(),
            DeleteAccountPath { id: account_id },
        )
        .await
        .unwrap();
        // 第二次也成功（幂等）
        let response = execute(
            &state.pg_pool,
            tests::test_operator_context(),
            DeleteAccountPath { id: account_id },
        )
        .await
        .unwrap();
        assert_eq!(response.id, account_id);
        assert!(response.deleted);

        // 幂等删除：只有第一条产生变更记录
        let mut conn = state.pg_pool.acquire().await.unwrap();
        let count = sqlx::query!(
            r#"SELECT COUNT(*) AS "count!" FROM audit_logs WHERE entity_id = $1"#,
            *account_id
        )
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        assert_eq!(count.count, 1);
    }

    #[sqlx::test]
    async fn test_delete_nonexistent_ok(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool).await;
        // 删除不存在的 ID 也返回成功（幂等语义），且不产生变更记录
        let response = execute(
            &state.pg_pool,
            tests::test_operator_context(),
            DeleteAccountPath {
                id: ID::from(999_i64),
            },
        )
        .await
        .unwrap();
        assert_eq!(response.id, ID::from(999_i64));
        assert!(response.deleted);

        let mut conn = state.pg_pool.acquire().await.unwrap();
        let count = sqlx::query!(
            r#"SELECT COUNT(*) AS "count!" FROM audit_logs WHERE entity_id = $1"#,
            999i64
        )
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        assert_eq!(count.count, 0);
    }
}

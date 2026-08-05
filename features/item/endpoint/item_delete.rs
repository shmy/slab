use audit_contract::AuditService;
use axum::extract::State;
use db::PgPool;
use http_auth::extract::operator::OperatorContext;
use item_contract::port::ItemPort;
use serde::{Deserialize, Serialize};
use shared_contract::value_object::id::ID;
use sqlx::Acquire;
use utoipa::{IntoParams, ToSchema};
use validify::Validify;
use web::extract::valid_path::ValidPath;
use web::response::json_response::{JsonResponse, JsonResponseType};

use crate::repository::item_repository::ItemRepository;

#[derive(Debug, Deserialize, Validify, IntoParams)]
#[into_params(parameter_in = Path)]
pub(crate) struct DeleteItemPath {
    pub id: ID,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct DeleteItemResponse {
    pub deleted: bool,
}

#[utoipa::path(
    delete,
    path = "/api/v1/items/{id}",
    operation_id = "item_delete",
    tag = "item",
    params(DeleteItemPath),
    responses((status = 200, body = JsonResponse<DeleteItemResponse>)),
    security(("bearerAuth" = []))
)]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ctx: OperatorContext,
    ValidPath(path): ValidPath<DeleteItemPath>,
) -> JsonResponseType<DeleteItemResponse> {
    let response = execute(&pg_pool, ctx, path).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip(pg_pool))]
#[inline]
async fn execute(
    pg_pool: &PgPool,
    ctx: OperatorContext,
    path: DeleteItemPath,
) -> rootcause::Result<DeleteItemResponse> {
    let mut conn = pg_pool.acquire().await?;
    let mut txn = conn.begin().await?;
    // 删除前读旧值用于变更历史；行不存在时不产生记录（幂等）
    let before = ItemPort::by_id(&mut txn, &path.id).await?;
    let deleted = ItemRepository::delete(&mut txn, &path.id).await?;
    if let Some(before) = before {
        AuditService::record_deleted(&mut txn, "item", &path.id, &ctx, &before).await?;
    }
    txn.commit().await?;
    Ok(DeleteItemResponse { deleted })
}

#[cfg(test)]
mod tests {
    use crate::tests;

    use super::*;
    use appctx::testing;
    use migration::run_migrations;

    #[sqlx::test]
    async fn test_delete_success(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool).await;

        // seed item
        let item_id = ID::new();
        let mut conn = state.pg_pool.acquire().await.unwrap();
        sqlx::query!(
            r#"INSERT INTO items (id, code, name, item_type, base_unit, version)
               VALUES ($1, 'RAW-000002', '待删物料', 1, 'kg', 1)"#,
            &*item_id
        )
        .execute(&mut *conn)
        .await
        .unwrap();
        drop(conn);

        let response = execute(
            &state.pg_pool,
            tests::test_operator_context(),
            DeleteItemPath { id: item_id },
        )
        .await
        .unwrap();
        assert!(response.deleted);

        // 变更历史：delete 类型，after 为空
        let mut conn = state.pg_pool.acquire().await.unwrap();
        let audit_row = sqlx::query!(
            r#"SELECT action, before, after FROM audit_logs WHERE entity_id = $1"#,
            *item_id
        )
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        assert_eq!(audit_row.action, 3); // Deleted
        let before: serde_json::Value = audit_row.before.unwrap();
        assert_eq!(before["name"], "待删物料");
        assert!(audit_row.after.is_none());

        // 幂等：再删一次（行已软删仍存在，但行存在即记录一次）
        execute(
            &state.pg_pool,
            tests::test_operator_context(),
            DeleteItemPath { id: item_id },
        )
        .await
        .unwrap();
        let count = sqlx::query!(
            r#"SELECT COUNT(*) AS "count!" FROM audit_logs WHERE entity_id = $1"#,
            *item_id
        )
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        assert_eq!(count.count, 2);
    }

    #[sqlx::test]
    async fn test_delete_nonexistent_no_audit(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool).await;
        let response = execute(
            &state.pg_pool,
            tests::test_operator_context(),
            DeleteItemPath {
                id: ID::from(999_i64),
            },
        )
        .await
        .unwrap();
        assert!(!response.deleted);

        // 不存在的行：不产生变更记录
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

use audit_contract::AuditService;
use axum::extract::State;
use db::PgPool;
use http_auth::extract::operator::OperatorContext;
use serde::{Deserialize, Serialize};
use shared_contract::value_object::id::ID;
use sqlx::Acquire;
use supplier_contract::port::SupplierPort;
use utoipa::{IntoParams, ToSchema};
use validify::Validify;
use web::extract::valid_path::ValidPath;
use web::response::json_response::{JsonResponse, JsonResponseType};

use crate::repository::supplier_repository::SupplierRepository;

#[derive(Debug, Deserialize, Validify, IntoParams)]
#[into_params(parameter_in = Path)]
pub(crate) struct DeleteSupplierPath {
    pub id: ID,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct DeleteSupplierResponse {
    pub deleted: bool,
}

#[utoipa::path(
    delete, path = "/api/v1/suppliers/{id}", operation_id = "supplier_delete", tag = "supplier",
    params(DeleteSupplierPath),
    responses((status = 200, body = JsonResponse<DeleteSupplierResponse>)),
    security(("bearerAuth" = []))
)]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ctx: OperatorContext,
    ValidPath(path): ValidPath<DeleteSupplierPath>,
) -> JsonResponseType<DeleteSupplierResponse> {
    let response = execute(&pg_pool, ctx, path).await?;
    JsonResponse::ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests;
    use appctx::testing;
    use migration::run_migrations;

    #[sqlx::test]
    async fn test_delete_success(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool).await;
        let supplier_id = tests::insert_test_supplier(&state.pg_pool).await;
        let response = execute(
            &state.pg_pool,
            tests::test_operator_context(),
            DeleteSupplierPath { id: supplier_id },
        )
        .await
        .unwrap();
        assert!(response.deleted);

        // 软删除生效：is_active 已置为 FALSE
        let mut conn = state.pg_pool.acquire().await.unwrap();
        let row = sqlx::query!(
            r#"SELECT is_active FROM suppliers WHERE id = $1"#,
            &*supplier_id
        )
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        assert!(!row.is_active);

        // 变更历史：delete 类型，after 为空
        let audit_row = sqlx::query!(
            r#"SELECT action, before, after FROM audit_logs WHERE entity_id = $1"#,
            *supplier_id
        )
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        assert_eq!(audit_row.action, 3); // Deleted
        let before: serde_json::Value = audit_row.before.unwrap();
        assert_eq!(before["name"], "Test Supplier");
        assert!(audit_row.after.is_none());
    }

    #[sqlx::test]
    async fn test_delete_nonexistent_ok(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool).await;
        // 删除不存在的 ID 返回成功（幂等语义），且不产生变更记录
        let response = execute(
            &state.pg_pool,
            tests::test_operator_context(),
            DeleteSupplierPath {
                id: ID::from(999_i64),
            },
        )
        .await
        .unwrap();
        assert!(!response.deleted);

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

#[tracing::instrument(skip(pg_pool))]
#[inline]
async fn execute(
    pg_pool: &PgPool,
    ctx: OperatorContext,
    path: DeleteSupplierPath,
) -> rootcause::Result<DeleteSupplierResponse> {
    let mut conn = pg_pool.acquire().await?;
    let mut txn = conn.begin().await?;

    // 删除前读旧值用于变更历史；供应商不存在时幂等删除，不产生记录
    let before = SupplierPort::by_id(&mut txn, &path.id).await?;
    let deleted = SupplierRepository::delete(txn.as_mut(), &path.id).await?;
    if let Some(before) = before {
        AuditService::record_deleted(&mut txn, "supplier", &path.id, &ctx, &before).await?;
    }
    txn.commit().await?;
    Ok(DeleteSupplierResponse { deleted })
}

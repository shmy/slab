use crate::repository::purchase_order_repository::PurchaseOrderRepository;
use audit_contract::AuditService;
use axum::extract::State;
use db::PgPool;
use http_auth::extract::operator::OperatorContext;
use purchase_contract::entity::PurchaseOrder;
use purchase_contract::error::PurchaseError;
use serde::{Deserialize, Serialize};
use shared_contract::value_object::id::ID;
use sqlx::Acquire;
use utoipa::{IntoParams, ToSchema};
use validify::Validify;
use web::extract::valid_path::ValidPath;
use web::response::json_response::{JsonResponse, JsonResponseType};

#[derive(Debug, Deserialize, Validify, IntoParams)]
#[into_params(parameter_in = Path)]
pub(crate) struct DeletePurchaseOrderPath {
    pub id: ID,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct DeletePurchaseOrderResponse {
    pub deleted: bool,
}

#[utoipa::path(
    delete,
    path = "/api/v1/purchase-orders/{id}",
    operation_id = "purchase_order_delete",
    tag = "purchase-order",
    params(DeletePurchaseOrderPath),
    responses((status = 200, body = JsonResponse<DeletePurchaseOrderResponse>)),
    security(("bearerAuth" = []))
)]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ctx: OperatorContext,
    ValidPath(path): ValidPath<DeletePurchaseOrderPath>,
) -> JsonResponseType<DeletePurchaseOrderResponse> {
    let response = execute(&pg_pool, ctx, path).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip(pg_pool))]
#[inline]
async fn execute(
    pg_pool: &PgPool,
    ctx: OperatorContext,
    path: DeletePurchaseOrderPath,
) -> rootcause::Result<DeletePurchaseOrderResponse> {
    let mut conn = pg_pool.acquire().await?;
    let mut txn = conn.begin().await?;

    // 删除前锁定读全行作为变更历史快照；只允许删除草稿状态（FOR UPDATE 保证并发安全）
    let before = sqlx::query_as!(
        PurchaseOrder,
        r#"SELECT id AS "id: ID", code, supplier_id, status, order_date,
                  expected_delivery_date, currency, total_amount,
                  payment_terms, remark, created_by AS "created_by: ID"
           FROM purchase_orders WHERE id = $1 FOR UPDATE"#,
        &*path.id
    )
    .fetch_optional(&mut *txn)
    .await?
    .ok_or(PurchaseError::NotFound)?;
    if before.status != 0 {
        return Err(PurchaseError::NotDraft.into());
    }

    // 软删除
    PurchaseOrderRepository::update_status(&mut txn, &path.id, -1).await?;
    AuditService::record_deleted(&mut txn, "purchase_order", &path.id, &ctx, &before).await?;

    txn.commit().await?;
    Ok(DeletePurchaseOrderResponse { deleted: true })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests;
    use appctx::testing;
    use migration::run_migrations;

    #[sqlx::test]
    async fn test_delete_draft_success(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool.clone()).await;
        let id = tests::insert_test_purchase_order(&state.pg_pool, "PO-DELETE-1", 0).await;

        let resp = execute(
            &state.pg_pool,
            crate::tests::test_operator_context(),
            DeletePurchaseOrderPath { id },
        )
        .await
        .unwrap();
        assert!(resp.deleted);

        let mut conn = state.pg_pool.acquire().await.unwrap();
        let status = sqlx::query_scalar!("SELECT status FROM purchase_orders WHERE id = $1", &*id)
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        assert_eq!(status, -1);

        // 变更历史：delete 类型，after 为空，快照为删除前状态
        let audit_row = sqlx::query!(
            r#"SELECT action, before, after FROM audit_logs WHERE entity_id = $1"#,
            *id
        )
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        assert_eq!(audit_row.action, 3); // Deleted
        let before: serde_json::Value = audit_row.before.unwrap();
        assert_eq!(before["status"], 0);
        assert!(audit_row.after.is_none());
    }

    #[sqlx::test]
    async fn test_delete_non_draft_rejected(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool.clone()).await;
        let id = tests::insert_test_purchase_order(&state.pg_pool, "PO-DELETE-2", 1).await;

        let err = execute(
            &state.pg_pool,
            crate::tests::test_operator_context(),
            DeletePurchaseOrderPath { id },
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("purchase_order_not_draft"));
    }

    #[sqlx::test]
    async fn test_delete_not_found(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool.clone()).await;

        let err = execute(
            &state.pg_pool,
            crate::tests::test_operator_context(),
            DeletePurchaseOrderPath { id: ID::new() },
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("purchase_order_not_found"));
    }
}

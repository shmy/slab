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
pub(crate) struct ApprovePurchaseOrderPath {
    pub id: ID,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct ApprovePurchaseOrderResponse {
    pub approved: bool,
    pub status: i16,
}

#[utoipa::path(
    post,
    path = "/api/v1/purchase-orders/{id}/approve",
    operation_id = "purchase_order_approve",
    tag = "purchase-order",
    params(ApprovePurchaseOrderPath),
    responses((status = 200, body = JsonResponse<ApprovePurchaseOrderResponse>)),
    security(("bearerAuth" = []))
)]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ctx: OperatorContext,
    ValidPath(path): ValidPath<ApprovePurchaseOrderPath>,
) -> JsonResponseType<ApprovePurchaseOrderResponse> {
    let response = execute(&pg_pool, ctx, path).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip(pg_pool))]
#[inline]
async fn execute(
    pg_pool: &PgPool,
    ctx: OperatorContext,
    path: ApprovePurchaseOrderPath,
) -> rootcause::Result<ApprovePurchaseOrderResponse> {
    let mut conn = pg_pool.acquire().await?;
    let mut txn = conn.begin().await?;

    // 变更历史：状态机前锁读全行作为 before，成功后同事务读回 after
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

    let new_status = PurchaseOrderRepository::approve(&mut txn, &path.id).await?;

    let after = sqlx::query_as!(
        PurchaseOrder,
        r#"SELECT id AS "id: ID", code, supplier_id, status, order_date,
                  expected_delivery_date, currency, total_amount,
                  payment_terms, remark, created_by AS "created_by: ID"
           FROM purchase_orders WHERE id = $1"#,
        &*path.id
    )
    .fetch_one(&mut *txn)
    .await?;
    AuditService::record_updated(&mut txn, "purchase_order", &path.id, &ctx, &before, &after)
        .await?;

    txn.commit().await?;
    Ok(ApprovePurchaseOrderResponse {
        approved: true,
        status: new_status,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests;
    use appctx::testing;
    use migration::run_migrations;
    use purchase_contract::value_object::PurchaseOrderStatus;

    #[sqlx::test]
    async fn test_approve_pending_success(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool.clone()).await;
        let id = tests::insert_test_purchase_order(&state.pg_pool, "PO-APPROVE-1", 1).await;

        let resp = execute(
            &state.pg_pool,
            crate::tests::test_operator_context(),
            ApprovePurchaseOrderPath { id },
        )
        .await
        .unwrap();
        assert!(resp.approved);
        assert_eq!(resp.status, PurchaseOrderStatus::PendingSupervisor as i16);

        let mut conn = state.pg_pool.acquire().await.unwrap();
        let row = sqlx::query!(
            "SELECT status, approved_at FROM purchase_orders WHERE id = $1",
            &*id
        )
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        assert_eq!(row.status, PurchaseOrderStatus::PendingSupervisor as i16);
        assert!(row.approved_at.is_some());

        // 变更历史：update 类型，before/after 快照状态分别为 1 → 2
        let audit_row = sqlx::query!(
            r#"SELECT action, before, after FROM audit_logs WHERE entity_id = $1"#,
            *id
        )
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        assert_eq!(audit_row.action, 2); // Updated
        let before: serde_json::Value = audit_row.before.unwrap();
        let after: serde_json::Value = audit_row.after.unwrap();
        assert_eq!(before["status"], PurchaseOrderStatus::Submitted as i16);
        assert_eq!(
            after["status"],
            PurchaseOrderStatus::PendingSupervisor as i16
        );
    }

    #[sqlx::test]
    async fn test_approve_draft_rejected(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool.clone()).await;
        let id = tests::insert_test_purchase_order(&state.pg_pool, "PO-APPROVE-2", 0).await;

        let err = execute(
            &state.pg_pool,
            crate::tests::test_operator_context(),
            ApprovePurchaseOrderPath { id },
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("invalid_status_transition"));
    }

    #[sqlx::test]
    async fn test_approve_not_found(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool.clone()).await;

        let err = execute(
            &state.pg_pool,
            crate::tests::test_operator_context(),
            ApprovePurchaseOrderPath { id: ID::new() },
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("purchase_order_not_found"));
    }
}

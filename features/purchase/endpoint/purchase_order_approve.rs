use crate::repository::purchase_order_repository::PurchaseOrderRepository;
use axum::extract::State;
use db::PgPool;
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
    ValidPath(path): ValidPath<ApprovePurchaseOrderPath>,
) -> JsonResponseType<ApprovePurchaseOrderResponse> {
    let response = execute(&pg_pool, path).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip(pg_pool))]
#[inline]
async fn execute(
    pg_pool: &PgPool,
    path: ApprovePurchaseOrderPath,
) -> rootcause::Result<ApprovePurchaseOrderResponse> {
    let mut conn = pg_pool.acquire().await?;
    let mut txn = conn.begin().await?;

    let new_status = PurchaseOrderRepository::approve(&mut txn, &path.id).await?;

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

    #[sqlx::test]
    async fn test_approve_pending_success(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool.clone()).await;
        let id = tests::insert_test_purchase_order(&state.pg_pool, "PO-APPROVE-1", 1).await;

        let resp = execute(&state.pg_pool, ApprovePurchaseOrderPath { id })
            .await
            .unwrap();
        assert!(resp.approved);
        assert_eq!(resp.status, 2);

        let row = sqlx::query!(
            "SELECT status, approved_at FROM purchase_orders WHERE id = $1",
            &*id
        )
        .fetch_one(&mut *state.pg_pool.acquire().await.unwrap())
        .await
        .unwrap();
        assert_eq!(row.status, 2);
        assert!(row.approved_at.is_some());
    }

    #[sqlx::test]
    async fn test_approve_draft_rejected(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool.clone()).await;
        let id = tests::insert_test_purchase_order(&state.pg_pool, "PO-APPROVE-2", 0).await;

        let err = execute(&state.pg_pool, ApprovePurchaseOrderPath { id })
            .await
            .unwrap_err();
        assert!(err.to_string().contains("invalid_status_transition"));
    }

    #[sqlx::test]
    async fn test_approve_not_found(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool.clone()).await;

        let err = execute(&state.pg_pool, ApprovePurchaseOrderPath { id: ID::new() })
            .await
            .unwrap_err();
        assert!(err.to_string().contains("purchase_order_not_found"));
    }
}

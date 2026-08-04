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
pub(crate) struct SubmitPurchaseOrderPath {
    pub id: ID,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct SubmitPurchaseOrderResponse {
    pub submitted: bool,
}

#[utoipa::path(
    post,
    path = "/api/v1/purchase-orders/{id}/submit",
    operation_id = "purchase_order_submit",
    tag = "purchase-order",
    params(SubmitPurchaseOrderPath),
    responses((status = 200, body = JsonResponse<SubmitPurchaseOrderResponse>)),
    security(("bearerAuth" = []))
)]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ValidPath(path): ValidPath<SubmitPurchaseOrderPath>,
) -> JsonResponseType<SubmitPurchaseOrderResponse> {
    let response = execute(&pg_pool, path).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip(pg_pool))]
#[inline]
async fn execute(
    pg_pool: &PgPool,
    path: SubmitPurchaseOrderPath,
) -> rootcause::Result<SubmitPurchaseOrderResponse> {
    let mut conn = pg_pool.acquire().await?;
    let mut txn = conn.begin().await?;

    let _ = PurchaseOrderRepository::submit(&mut txn, &path.id).await?;

    txn.commit().await?;
    Ok(SubmitPurchaseOrderResponse { submitted: true })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests;
    use appctx::testing;
    use migration::run_migrations;

    #[sqlx::test]
    async fn test_submit_draft_success(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool.clone()).await;
        let id = tests::insert_test_purchase_order(&state.pg_pool, "PO-SUBMIT-1", 0).await;

        let resp = execute(&state.pg_pool, SubmitPurchaseOrderPath { id })
            .await
            .unwrap();
        assert!(resp.submitted);

        let status = sqlx::query_scalar!("SELECT status FROM purchase_orders WHERE id = $1", &*id)
            .fetch_one(&mut *state.pg_pool.acquire().await.unwrap())
            .await
            .unwrap();
        assert_eq!(status, 1);
    }

    #[sqlx::test]
    async fn test_submit_already_submitted_rejected(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool.clone()).await;
        let id = tests::insert_test_purchase_order(&state.pg_pool, "PO-SUBMIT-2", 1).await;

        let err = execute(&state.pg_pool, SubmitPurchaseOrderPath { id })
            .await
            .unwrap_err();
        assert!(err.to_string().contains("invalid_status_transition"));
    }

    #[sqlx::test]
    async fn test_submit_not_found(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool.clone()).await;

        let err = execute(&state.pg_pool, SubmitPurchaseOrderPath { id: ID::new() })
            .await
            .unwrap_err();
        assert!(err.to_string().contains("purchase_order_not_found"));
    }
}

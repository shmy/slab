use crate::repository::sales_order_repository::SalesOrderRepository;
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
pub(crate) struct SalesActionPath {
    pub id: ID,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct SalesActionResponse {
    pub success: bool,
}

#[utoipa::path(
    post, path = "/api/v1/sales-orders/{id}/submit",
    operation_id = "sales_order_submit", tag = "sales-order",
    params(SalesActionPath),
    responses((status = 200, body = JsonResponse<SalesActionResponse>)),
    security(("bearerAuth" = []))
)]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn submit_handler(
    State(pg_pool): State<PgPool>,
    ValidPath(path): ValidPath<SalesActionPath>,
) -> JsonResponseType<SalesActionResponse> {
    let response = submit_execute(&pg_pool, path).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip_all)]
#[inline]
async fn submit_execute(
    pg_pool: &PgPool,
    path: SalesActionPath,
) -> rootcause::Result<SalesActionResponse> {
    let mut conn = pg_pool.acquire().await?;
    let mut txn = conn.begin().await?;

    SalesOrderRepository::submit(&mut txn, &path.id).await?;

    txn.commit().await?;
    Ok(SalesActionResponse { success: true })
}

#[utoipa::path(
    post, path = "/api/v1/sales-orders/{id}/approve",
    operation_id = "sales_order_approve", tag = "sales-order",
    params(SalesActionPath),
    responses((status = 200, body = JsonResponse<SalesActionResponse>)),
    security(("bearerAuth" = [])))]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn approve_handler(
    State(pg_pool): State<PgPool>,
    ValidPath(path): ValidPath<SalesActionPath>,
) -> JsonResponseType<SalesActionResponse> {
    let response = approve_execute(&pg_pool, path).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip_all)]
#[inline]
async fn approve_execute(
    pg_pool: &PgPool,
    path: SalesActionPath,
) -> rootcause::Result<SalesActionResponse> {
    let mut conn = pg_pool.acquire().await?;
    let mut txn = conn.begin().await?;

    SalesOrderRepository::approve(&mut txn, &path.id).await?;

    txn.commit().await?;
    Ok(SalesActionResponse { success: true })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests;
    use appctx::testing;
    use migration::run_migrations;

    async fn seed_order(state: &appctx::AppCtx, code: &str, status: i16) -> ID {
        let customer_id = tests::insert_test_customer(&state.pg_pool, "C-AP-1").await;
        tests::insert_test_sales_order(&state.pg_pool, code, &customer_id, status).await
    }

    #[sqlx::test]
    async fn test_submit_draft_success(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool.clone()).await;
        let id = seed_order(&state, "SO-AP-1", 0).await;

        let resp = submit_execute(&state.pg_pool, SalesActionPath { id })
            .await
            .unwrap();
        assert!(resp.success);

        let status = sqlx::query_scalar!("SELECT status FROM sales_orders WHERE id = $1", &*id)
            .fetch_one(&mut *state.pg_pool.acquire().await.unwrap())
            .await
            .unwrap();
        assert_eq!(status, 1);
    }

    #[sqlx::test]
    async fn test_submit_non_draft_rejected(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool.clone()).await;
        let id = seed_order(&state, "SO-AP-2", 1).await;

        let err = submit_execute(&state.pg_pool, SalesActionPath { id })
            .await
            .unwrap_err();
        assert!(err.to_string().contains("invalid_status_transition"));
    }

    #[sqlx::test]
    async fn test_approve_pending_success(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool.clone()).await;
        let id = seed_order(&state, "SO-AP-3", 1).await;

        let resp = approve_execute(&state.pg_pool, SalesActionPath { id })
            .await
            .unwrap();
        assert!(resp.success);

        let row = sqlx::query!(
            "SELECT status, approved_at FROM sales_orders WHERE id = $1",
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
        let id = seed_order(&state, "SO-AP-4", 0).await;

        let err = approve_execute(&state.pg_pool, SalesActionPath { id })
            .await
            .unwrap_err();
        assert!(err.to_string().contains("invalid_status_transition"));
    }

    #[sqlx::test]
    async fn test_submit_not_found(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool.clone()).await;

        let err = submit_execute(&state.pg_pool, SalesActionPath { id: ID::new() })
            .await
            .unwrap_err();
        assert!(err.to_string().contains("sales_document_not_found"));
    }
}

use crate::repository::sales_order_repository::SalesOrderRepository;
use audit_contract::AuditService;
use axum::extract::State;
use db::PgPool;
use http_auth::extract::operator::OperatorContext;
use sales_contract::entity::SalesOrder;
use sales_contract::error::SalesError;
use serde::{Deserialize, Serialize};
use shared_contract::value_object::id::ID;
use sqlx::Acquire;
use sqlx::PgConnection;
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
    ctx: OperatorContext,
    ValidPath(path): ValidPath<SalesActionPath>,
) -> JsonResponseType<SalesActionResponse> {
    let response = submit_execute(&pg_pool, ctx, path).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip_all)]
#[inline]
async fn submit_execute(
    pg_pool: &PgPool,
    ctx: OperatorContext,
    path: SalesActionPath,
) -> rootcause::Result<SalesActionResponse> {
    let mut conn = pg_pool.acquire().await?;
    let mut txn = conn.begin().await?;

    // 变更历史：状态机变更前锁读整行（与仓库内部锁同一行，重入无碍）
    let before = lock_read_sales_order(&mut txn, &path.id).await?;
    SalesOrderRepository::submit(&mut txn, &path.id).await?;
    // 事务内重读（可见自身未提交写入）作为变更后快照
    let after = lock_read_sales_order(&mut txn, &path.id).await?;
    AuditService::record_updated(&mut txn, "sales_order", &path.id, &ctx, &before, &after).await?;

    txn.commit().await?;
    Ok(SalesActionResponse { success: true })
}

/// 同事务锁读销售订单整行（FOR UPDATE）。
async fn lock_read_sales_order(conn: &mut PgConnection, id: &ID) -> rootcause::Result<SalesOrder> {
    let row = sqlx::query!(
        r#"SELECT id, code, customer_id, status, order_date, currency, total_amount, remark, created_by
           FROM sales_orders WHERE id = $1 FOR UPDATE"#,
        id as _
    )
    .fetch_optional(conn)
    .await?
    .ok_or(SalesError::NotFound)?;
    Ok(SalesOrder {
        id: ID::new_unchecked(row.id),
        code: row.code,
        customer_id: ID::new_unchecked(row.customer_id),
        status: row.status,
        order_date: row.order_date,
        currency: row.currency,
        total_amount: row.total_amount,
        remark: row.remark,
        created_by: row.created_by.map(ID::new_unchecked),
    })
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
    ctx: OperatorContext,
    ValidPath(path): ValidPath<SalesActionPath>,
) -> JsonResponseType<SalesActionResponse> {
    let response = approve_execute(&pg_pool, ctx, path).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip_all)]
#[inline]
async fn approve_execute(
    pg_pool: &PgPool,
    ctx: OperatorContext,
    path: SalesActionPath,
) -> rootcause::Result<SalesActionResponse> {
    let mut conn = pg_pool.acquire().await?;
    let mut txn = conn.begin().await?;

    // 变更历史：状态机变更前锁读整行（与仓库内部锁同一行，重入无碍）
    let before = lock_read_sales_order(&mut txn, &path.id).await?;
    SalesOrderRepository::approve(&mut txn, &path.id).await?;
    // 事务内重读（可见自身未提交写入）作为变更后快照
    let after = lock_read_sales_order(&mut txn, &path.id).await?;
    AuditService::record_updated(&mut txn, "sales_order", &path.id, &ctx, &before, &after).await?;

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

        let resp = submit_execute(
            &state.pg_pool,
            tests::test_operator_context(),
            SalesActionPath { id },
        )
        .await
        .unwrap();
        assert!(resp.success);

        let status = sqlx::query_scalar!("SELECT status FROM sales_orders WHERE id = $1", &*id)
            .fetch_one(&mut *state.pg_pool.acquire().await.unwrap())
            .await
            .unwrap();
        assert_eq!(status, 1);

        // 变更历史：update 类型，before.status=0 → after.status=1
        let audit_row = sqlx::query!(
            r#"SELECT action, entity, before, after FROM audit_logs WHERE entity_id = $1"#,
            *id
        )
        .fetch_one(&mut *state.pg_pool.acquire().await.unwrap())
        .await
        .unwrap();
        assert_eq!(audit_row.action, 2); // Updated
        assert_eq!(audit_row.entity, "sales_order");
        let before: serde_json::Value = audit_row.before.unwrap();
        let after: serde_json::Value = audit_row.after.unwrap();
        assert_eq!(before["status"], 0);
        assert_eq!(after["status"], 1);
    }

    #[sqlx::test]
    async fn test_submit_non_draft_rejected(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool.clone()).await;
        let id = seed_order(&state, "SO-AP-2", 1).await;

        let err = submit_execute(
            &state.pg_pool,
            tests::test_operator_context(),
            SalesActionPath { id },
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("invalid_status_transition"));
    }

    #[sqlx::test]
    async fn test_approve_pending_success(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool.clone()).await;
        let id = seed_order(&state, "SO-AP-3", 1).await;

        let resp = approve_execute(
            &state.pg_pool,
            tests::test_operator_context(),
            SalesActionPath { id },
        )
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

        // 变更历史：update 类型，before.status=1 → after.status=2
        let audit_row = sqlx::query!(
            r#"SELECT action, before, after FROM audit_logs WHERE entity_id = $1"#,
            *id
        )
        .fetch_one(&mut *state.pg_pool.acquire().await.unwrap())
        .await
        .unwrap();
        assert_eq!(audit_row.action, 2); // Updated
        let before: serde_json::Value = audit_row.before.unwrap();
        let after: serde_json::Value = audit_row.after.unwrap();
        assert_eq!(before["status"], 1);
        assert_eq!(after["status"], 2);
    }

    #[sqlx::test]
    async fn test_approve_draft_rejected(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool.clone()).await;
        let id = seed_order(&state, "SO-AP-4", 0).await;

        let err = approve_execute(
            &state.pg_pool,
            tests::test_operator_context(),
            SalesActionPath { id },
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("invalid_status_transition"));
    }

    #[sqlx::test]
    async fn test_submit_not_found(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool.clone()).await;

        let err = submit_execute(
            &state.pg_pool,
            tests::test_operator_context(),
            SalesActionPath { id: ID::new() },
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("sales_document_not_found"));
    }
}

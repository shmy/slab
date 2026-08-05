//! 提交调拨单。

use crate::repository::stock_transfer_repository::StockTransferRepository;
use crate::shared::snapshot::StockTransferSnapshot;
use audit_contract::AuditService;
use axum::extract::State;
use db::PgPool;
use http_auth::extract::operator::OperatorContext;
use serde::{Deserialize, Serialize};
use shared_contract::value_object::id::ID;
use sqlx::Acquire;
use utoipa::{IntoParams, ToSchema};
use validify::Validify;
use warehouse_contract::error::WarehouseError;
use web::extract::valid_path::ValidPath;
use web::response::json_response::{JsonResponse, JsonResponseType};

#[derive(Debug, Deserialize, Validify, IntoParams)]
#[into_params(parameter_in = Path)]
pub(crate) struct TransferActionPath {
    pub id: ID,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct TransferActionResponse {
    pub success: bool,
}

#[utoipa::path(post, path = "/api/v1/stock-transfers/{id}/submit",
    operation_id = "stock_transfer_submit", tag = "stock-transfer",
    params(TransferActionPath),
    responses((status = 200, body = JsonResponse<TransferActionResponse>)),
    security(("bearerAuth" = [])))]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ctx: OperatorContext,
    ValidPath(path): ValidPath<TransferActionPath>,
) -> JsonResponseType<TransferActionResponse> {
    let response = execute(&pg_pool, ctx, path).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip_all)]
#[inline]
async fn execute(
    pg_pool: &PgPool,
    ctx: OperatorContext,
    path: TransferActionPath,
) -> rootcause::Result<TransferActionResponse> {
    let mut conn = pg_pool.acquire().await?;
    let mut txn = conn.begin().await?;

    // 变更历史：先锁读整行作为 before；仓库方法内部 FOR UPDATE 为同行的可重入锁
    let before = StockTransferSnapshot::read_locked(&mut txn, &path.id)
        .await?
        .ok_or(WarehouseError::NotFound)?;
    StockTransferRepository::submit(&mut txn, &path.id).await?;
    // 写后同事务读回整行作为 after
    let after = StockTransferSnapshot::read(&mut txn, &path.id)
        .await?
        .ok_or(WarehouseError::NotFound)?;
    AuditService::record_updated(&mut txn, "stock_transfer", &path.id, &ctx, &before, &after)
        .await?;

    txn.commit().await?;
    Ok(TransferActionResponse { success: true })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests;
    use appctx::testing;
    use migration::run_migrations;

    #[sqlx::test]
    async fn test_submit_success(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool).await;
        let from_wh = tests::insert_test_warehouse(&state.pg_pool, "TRF-S-A").await;
        let to_wh = tests::insert_test_warehouse(&state.pg_pool, "TRF-S-B").await;
        let id = ID::new();
        sqlx::query!(
            r#"INSERT INTO stock_transfers (id, code, from_warehouse_id, to_warehouse_id, status, transfer_date)
               VALUES ($1, $2, $3, $4, 0, CURRENT_DATE)"#,
            &*id,
            "TRF-S",
            &*from_wh,
            &*to_wh,
        )
        .execute(&mut *state.pg_pool.acquire().await.unwrap())
        .await
        .unwrap();

        let response = execute(
            &state.pg_pool,
            tests::test_operator_context(),
            TransferActionPath { id },
        )
        .await
        .unwrap();
        assert!(response.success);

        // 变更历史：update 类型，状态 Draft(0) → Submit(1)
        let mut conn = state.pg_pool.acquire().await.unwrap();
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
        assert_eq!(before["status"], 0);
        assert_eq!(after["status"], 1);
        assert_eq!(after["code"], "TRF-S");
    }
}

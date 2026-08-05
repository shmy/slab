//! 创建调拨单。

use crate::shared::snapshot::StockTransferSnapshot;
use audit_contract::AuditService;
use axum::extract::State;
use code_gen::CodeGen;
use db::PgPool;
use http_auth::extract::operator::OperatorContext;
use serde::{Deserialize, Serialize};
use shared_contract::value_object::id::ID;
use sqlx::Acquire;
use utoipa::ToSchema;
use validify::Validify;
use warehouse_contract::error::WarehouseError;
use web::extract::valid_json::ValidJson;
use web::response::json_response::{JsonResponse, JsonResponseType};

#[derive(Debug, Deserialize, Validify, ToSchema)]
pub(crate) struct TransferItemInput {
    pub item_id: ID,
    pub quantity: i64,
    pub batch_number: Option<String>,
}

#[derive(Debug, Deserialize, Validify, ToSchema)]
pub(crate) struct CreateTransferRequest {
    pub from_warehouse_id: ID,
    pub to_warehouse_id: ID,
    pub transfer_date: Option<chrono::NaiveDate>,
    pub remark: Option<String>,
    pub items: Vec<TransferItemInput>,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct CreateTransferResponse {
    pub id: ID,
    pub code: String,
}

#[utoipa::path(post, path = "/api/v1/stock-transfers",
    operation_id = "stock_transfer_create", tag = "stock-transfer",
    request_body = CreateTransferRequest,
    responses((status = 200, body = JsonResponse<CreateTransferResponse>)),
    security(("bearerAuth" = [])))]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ctx: OperatorContext,
    ValidJson(request): ValidJson<CreateTransferRequest>,
) -> JsonResponseType<CreateTransferResponse> {
    let response = execute(&pg_pool, ctx, request).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip_all)]
#[inline]
async fn execute(
    pg_pool: &PgPool,
    ctx: OperatorContext,
    request: CreateTransferRequest,
) -> rootcause::Result<CreateTransferResponse> {
    let mut conn = pg_pool.acquire().await?;
    let mut txn = conn.begin().await?;
    let code = CodeGen::next_code(&mut txn, "seq_stock_transfer", "TRF").await?;
    let id = ID::new();
    sqlx::query!(
        r#"INSERT INTO stock_transfers (id, code, from_warehouse_id, to_warehouse_id, transfer_date, remark, status)
           VALUES ($1, $2, $3, $4, $5, $6, 0)"#,
        &*id, code, &*request.from_warehouse_id, &*request.to_warehouse_id,
        request.transfer_date.unwrap_or_else(|| chrono::Utc::now().date_naive()), request.remark,
    ).execute(&mut *txn).await?;
    for item in &request.items {
        let line_id = ID::new();
        sqlx::query!(
            r#"INSERT INTO stock_transfer_items (id, transfer_id, item_id, quantity, batch_number)
               VALUES ($1, $2, $3, $4, $5)"#,
            &*line_id,
            &*id,
            &*item.item_id,
            item.quantity,
            item.batch_number,
        )
        .execute(&mut *txn)
        .await?;
    }
    // 写入未返回实体，同事务读回整行作为 after 快照
    let after = StockTransferSnapshot::read(&mut txn, &id)
        .await?
        .ok_or(WarehouseError::NotFound)?;
    AuditService::record_create(&mut txn, "stock_transfer", &id, &ctx, &after).await?;
    txn.commit().await?;
    Ok(CreateTransferResponse { id, code })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests;
    use appctx::testing;
    use migration::run_migrations;

    #[sqlx::test]
    async fn test_create_success(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool).await;
        let from_wh = tests::insert_test_warehouse(&state.pg_pool, "TRF-C-A").await;
        let to_wh = tests::insert_test_warehouse(&state.pg_pool, "TRF-C-B").await;
        let request = CreateTransferRequest {
            from_warehouse_id: from_wh,
            to_warehouse_id: to_wh,
            transfer_date: None,
            remark: Some("test transfer".into()),
            items: vec![],
        };
        let response = execute(&state.pg_pool, tests::test_operator_context(), request)
            .await
            .unwrap();
        assert!(i64::from(response.id) > 0);
        assert!(response.code.starts_with("TRF-"));

        // 变更历史：create 类型，before 为空，快照为草稿状态
        let mut conn = state.pg_pool.acquire().await.unwrap();
        let audit_row = sqlx::query!(
            r#"SELECT action, before, after FROM audit_logs WHERE entity_id = $1"#,
            *response.id
        )
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        assert_eq!(audit_row.action, 1); // Created
        assert!(audit_row.before.is_none());
        let after: serde_json::Value = audit_row.after.unwrap();
        assert_eq!(after["status"], 0);
        assert_eq!(after["code"], response.code.as_str());
        assert_eq!(after["remark"], "test transfer");
    }
}

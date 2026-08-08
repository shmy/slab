use audit_contract::AuditService;
use axum::extract::State;
use db::PgPool;
use doc_numbering::DocNumberer;
use http_auth::extract::operator::OperatorContext;
use inventory_ledger::{InventoryLedger, LedgerCommand, TransactionType};
use production_contract::entity::ProductionReceipt;
use production_contract::error::ProductionError;
use serde::{Deserialize, Serialize};
use shared_contract::value_object::id::ID;
use sqlx::Acquire;
use utoipa::ToSchema;
use validify::Validify;
use web::extract::valid_json::ValidJson;
use web::response::json_response::{JsonResponse, JsonResponseType};

#[derive(Debug, Deserialize, Validify, ToSchema)]
pub(crate) struct CreateReceiptRequest {
    pub work_order_id: ID,
    pub warehouse_id: ID,
    pub quantity: i64,
    pub batch_number: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct CreateReceiptResponse {
    pub id: ID,
    pub code: String,
}

#[utoipa::path(post, path = "/api/v1/production-receipts",
    operation_id = "production_receipt_create", tag = "production",
    request_body = CreateReceiptRequest,
    responses((status = 200, body = JsonResponse<CreateReceiptResponse>)),
    security(("bearerAuth" = [])))]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ctx: OperatorContext,
    ValidJson(request): ValidJson<CreateReceiptRequest>,
) -> JsonResponseType<CreateReceiptResponse> {
    let response = execute(&pg_pool, ctx, request).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip_all)]
#[inline]
async fn execute(
    pg_pool: &PgPool,
    ctx: OperatorContext,
    request: CreateReceiptRequest,
) -> rootcause::Result<CreateReceiptResponse> {
    let mut conn = pg_pool.acquire().await?;
    let mut txn = conn.begin().await?;

    let wo = sqlx::query!(
        "SELECT item_id, status FROM work_orders WHERE id = $1 FOR UPDATE",
        &*request.work_order_id
    )
    .fetch_optional(&mut *txn)
    .await?
    .ok_or(ProductionError::NotFound)?;

    let code = DocNumberer::next_number(&mut txn, "seq_production_receipt", "PR").await?;
    let id = ID::new();

    sqlx::query!(
        r#"INSERT INTO production_receipts (id, code, work_order_id, item_id, warehouse_id, quantity, batch_number)
           VALUES ($1, $2, $3, $4, $5, $6, $7)"#,
        &*id, code, &*request.work_order_id, wo.item_id, &*request.warehouse_id, request.quantity, request.batch_number,
    ).execute(&mut *txn).await?;

    InventoryLedger::receive(
        &mut txn,
        &LedgerCommand {
            item_id: &ID::new_unchecked(wo.item_id),
            warehouse_id: &request.warehouse_id,
            quantity: request.quantity,
            tx_type: TransactionType::Inbound,
            reference_type: "production_receipt",
            reference_id: &id,
            batch_number: request.batch_number.as_deref(),
        },
    )
    .await?;

    // 变更历史：创建完工入库单（同事务写，回滚即消失）
    let receipt = sqlx::query_as!(
        ProductionReceipt,
        r#"SELECT
               id as "id: ID",
               code,
               work_order_id as "work_order_id: ID",
               item_id as "item_id: ID",
               warehouse_id as "warehouse_id: ID",
               quantity,
               batch_number
           FROM production_receipts
           WHERE id = $1"#,
        &*id
    )
    .fetch_one(&mut *txn)
    .await?;
    AuditService::record_create(&mut txn, "production_receipt", &id, &ctx, &receipt).await?;

    txn.commit().await?;
    Ok(CreateReceiptResponse { id, code })
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

        let item_id = tests::insert_test_item(&state.pg_pool, "I-PR-1").await;
        let bom_id = tests::insert_test_bom(&state.pg_pool, "BOM-PR-1", &item_id).await;
        let wo_id = ID::new();
        let wh_id = ID::new();
        let mut conn = state.pg_pool.acquire().await.unwrap();

        sqlx::query!(
            r#"INSERT INTO work_orders (id, code, bom_id, item_id, planned_qty, status)
               VALUES ($1, $2, $3, $4, 10, 1)"#,
            &*wo_id,
            "MO-PR-1",
            &*bom_id,
            &*item_id,
        )
        .execute(&mut *conn)
        .await
        .unwrap();
        sqlx::query!(
            "INSERT INTO warehouses (id, code, name, type, is_active) VALUES ($1, 'WH-PR1', 'Main', 1, true)",
            &*wh_id
        )
        .execute(&mut *conn)
        .await
        .unwrap();

        let response = execute(
            &state.pg_pool,
            tests::test_operator_context(),
            CreateReceiptRequest {
                work_order_id: wo_id,
                warehouse_id: wh_id,
                quantity: 5,
                batch_number: Some("BATCH-1".to_string()),
            },
        )
        .await
        .unwrap();
        assert!(response.code.starts_with("PR-"));

        // 变更历史：create 类型，entity = production_receipt
        let audit_row = sqlx::query!(
            r#"SELECT entity, action, before, after FROM audit_logs WHERE entity_id = $1"#,
            *response.id
        )
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        assert_eq!(audit_row.entity, "production_receipt");
        assert_eq!(audit_row.action, 1); // Created
        assert!(audit_row.before.is_none());
        let after: serde_json::Value = audit_row.after.unwrap();
        assert_eq!(after["code"].as_str(), Some(response.code.as_str()));
        assert_eq!(after["quantity"], 5);
        assert_eq!(after["batch_number"], "BATCH-1");
    }

    #[sqlx::test]
    async fn test_create_work_order_not_found(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool).await;

        let err = execute(
            &state.pg_pool,
            tests::test_operator_context(),
            CreateReceiptRequest {
                work_order_id: ID::new(),
                warehouse_id: ID::new(),
                quantity: 5,
                batch_number: None,
            },
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("work_order_not_found"));
    }
}

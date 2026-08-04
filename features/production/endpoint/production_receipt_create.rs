use axum::extract::State;
use code_gen::CodeGen;
use db::PgPool;
use inventory_ledger::{InventoryLedger, LedgerCommand, TransactionType};
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
    ValidJson(request): ValidJson<CreateReceiptRequest>,
) -> JsonResponseType<CreateReceiptResponse> {
    let response = execute(&pg_pool, request).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip_all)]
#[inline]
async fn execute(
    pg_pool: &PgPool,
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

    let code = CodeGen::next_code(&mut txn, "seq_production_receipt", "PR").await?;
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

    txn.commit().await?;
    Ok(CreateReceiptResponse { id, code })
}

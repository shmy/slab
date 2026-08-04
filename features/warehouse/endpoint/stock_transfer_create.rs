//! 创建调拨单。

use axum::extract::State;
use code_gen::CodeGen;
use db::PgPool;
use serde::{Deserialize, Serialize};
use shared_contract::value_object::id::ID;
use sqlx::Acquire;
use utoipa::ToSchema;
use validify::Validify;
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
    ValidJson(request): ValidJson<CreateTransferRequest>,
) -> JsonResponseType<CreateTransferResponse> {
    let response = execute(&pg_pool, request).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip_all)]
#[inline]
async fn execute(
    pg_pool: &PgPool,
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
    txn.commit().await?;
    Ok(CreateTransferResponse { id, code })
}

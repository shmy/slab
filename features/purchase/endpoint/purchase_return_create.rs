use axum::extract::State;
use code_gen::CodeGen;
use db::PgPool;
use purchase_contract::error::PurchaseError;
use serde::{Deserialize, Serialize};
use shared_contract::value_object::id::ID;
use sqlx::Acquire;
use utoipa::ToSchema;
use validify::Validify;
use web::extract::valid_json::ValidJson;
use web::response::json_response::{JsonResponse, JsonResponseType};

#[derive(Debug, Deserialize, Validify, ToSchema)]
pub(crate) struct CreateReturnLine {
    pub receipt_line_id: ID,
    pub item_id: ID,
    pub quantity: i64,
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize, Validify, ToSchema)]
pub(crate) struct CreateReturnRequest {
    pub order_id: ID,
    pub reason: Option<String>,
    pub lines: Vec<CreateReturnLine>,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct CreateReturnResponse {
    pub id: ID,
    pub code: String,
}

#[utoipa::path(
    post,
    path = "/api/v1/purchase-returns",
    operation_id = "purchase_return_create",
    tag = "purchase-return",
    request_body = CreateReturnRequest,
    responses((status = 200, body = JsonResponse<CreateReturnResponse>)),
    security(("bearerAuth" = []))
)]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ValidJson(request): ValidJson<CreateReturnRequest>,
) -> JsonResponseType<CreateReturnResponse> {
    let response = execute(&pg_pool, request).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip_all)]
#[inline]
async fn execute(
    pg_pool: &PgPool,
    request: CreateReturnRequest,
) -> rootcause::Result<CreateReturnResponse> {
    let mut conn = pg_pool.acquire().await?;
    let mut txn = conn.begin().await?;

    // 验证订单存在
    let order = sqlx::query!(
        r#"SELECT supplier_id, status FROM purchase_orders WHERE id = $1"#,
        &*request.order_id
    )
    .fetch_optional(&mut *txn)
    .await?
    .ok_or(PurchaseError::NotFound)?;

    let code = CodeGen::next_code(&mut txn, "seq_purchase_return", "RET").await?;

    let return_id = ID::new();
    sqlx::query!(
        r#"INSERT INTO purchase_returns
               (id, code, order_id, supplier_id, reason, status)
           VALUES ($1, $2, $3, $4, $5, 0)"#,
        &*return_id,
        code,
        &*request.order_id,
        &*ID::new_unchecked(order.supplier_id),
        request.reason,
    )
    .execute(&mut *txn)
    .await?;

    for line in &request.lines {
        let line_id = ID::new();
        sqlx::query!(
            r#"INSERT INTO purchase_return_lines
                   (id, return_id, receipt_line_id, item_id, quantity, reason)
               VALUES ($1, $2, $3, $4, $5, $6)"#,
            &*line_id,
            &*return_id,
            &*line.receipt_line_id,
            &*line.item_id,
            line.quantity,
            line.reason,
        )
        .execute(&mut *txn)
        .await?;
    }

    txn.commit().await?;
    Ok(CreateReturnResponse {
        id: return_id,
        code,
    })
}

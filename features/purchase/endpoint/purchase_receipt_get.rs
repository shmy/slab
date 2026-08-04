use axum::extract::State;
use db::PgPool;
use purchase_contract::entity::{PurchaseReceipt, PurchaseReceiptLine};
use purchase_contract::error::PurchaseError;
use serde::{Deserialize, Serialize};
use shared_contract::value_object::id::ID;
use utoipa::{IntoParams, ToSchema};
use validify::Validify;
use web::extract::valid_path::ValidPath;
use web::response::json_response::{JsonResponse, JsonResponseType};

#[derive(Debug, Deserialize, Validify, IntoParams)]
#[into_params(parameter_in = Path)]
pub(crate) struct GetReceiptPath {
    pub id: ID,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct ReceiptDetail {
    pub receipt: PurchaseReceipt,
    pub lines: Vec<PurchaseReceiptLine>,
}

#[utoipa::path(
    get,
    path = "/api/v1/purchase-receipts/{id}",
    operation_id = "purchase_receipt_get",
    tag = "purchase-receipt",
    params(GetReceiptPath),
    responses((status = 200, body = JsonResponse<ReceiptDetail>)),
    security(("bearerAuth" = []))
)]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ValidPath(path): ValidPath<GetReceiptPath>,
) -> JsonResponseType<ReceiptDetail> {
    let response = execute(&pg_pool, path).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip_all)]
#[inline]
async fn execute(pg_pool: &PgPool, path: GetReceiptPath) -> rootcause::Result<ReceiptDetail> {
    let mut conn = pg_pool.acquire().await?;

    let row = sqlx::query!(
        r#"SELECT id, code, order_id, supplier_id, receipt_date, status, remark
           FROM purchase_receipts WHERE id = $1"#,
        &*path.id
    )
    .fetch_optional(&mut *conn)
    .await?
    .ok_or(PurchaseError::NotFound)?;

    let receipt = PurchaseReceipt {
        id: ID::new_unchecked(row.id),
        code: row.code,
        order_id: ID::new_unchecked(row.order_id),
        supplier_id: ID::new_unchecked(row.supplier_id),
        receipt_date: row.receipt_date,
        status: row.status,
        remark: row.remark,
    };

    let line_rows = sqlx::query!(
        r#"SELECT id, receipt_id, order_line_id, item_id, warehouse_id,
                  quantity, batch_number, unit_cost
           FROM purchase_receipt_lines WHERE receipt_id = $1"#,
        &*path.id
    )
    .fetch_all(&mut *conn)
    .await?;

    let lines = line_rows
        .into_iter()
        .map(|r| PurchaseReceiptLine {
            id: ID::new_unchecked(r.id),
            receipt_id: ID::new_unchecked(r.receipt_id),
            order_line_id: ID::new_unchecked(r.order_line_id),
            item_id: ID::new_unchecked(r.item_id),
            warehouse_id: ID::new_unchecked(r.warehouse_id),
            quantity: r.quantity,
            batch_number: r.batch_number,
            unit_cost: r.unit_cost,
        })
        .collect();

    Ok(ReceiptDetail { receipt, lines })
}

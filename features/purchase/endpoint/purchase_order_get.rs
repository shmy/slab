use axum::extract::State;
use db::PgPool;
use purchase_contract::entity::{PurchaseOrder, PurchaseOrderLine};
use purchase_contract::error::PurchaseError;
use serde::{Deserialize, Serialize};
use shared_contract::value_object::id::ID;
use utoipa::{IntoParams, ToSchema};
use validify::Validify;
use web::extract::valid_path::ValidPath;
use web::response::json_response::{JsonResponse, JsonResponseType};

#[derive(Debug, Deserialize, Validify, IntoParams)]
#[into_params(parameter_in = Path)]
pub(crate) struct GetPurchaseOrderPath {
    pub id: ID,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct PurchaseOrderDetail {
    pub order: PurchaseOrder,
    pub lines: Vec<PurchaseOrderLine>,
}

#[utoipa::path(
    get,
    path = "/api/v1/purchase-orders/{id}",
    operation_id = "purchase_order_get",
    tag = "purchase-order",
    params(GetPurchaseOrderPath),
    responses((status = 200, body = JsonResponse<PurchaseOrderDetail>)),
    security(("bearerAuth" = []))
)]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ValidPath(path): ValidPath<GetPurchaseOrderPath>,
) -> JsonResponseType<PurchaseOrderDetail> {
    let response = execute(&pg_pool, path).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip_all)]
#[inline]
async fn execute(
    pg_pool: &PgPool,
    path: GetPurchaseOrderPath,
) -> rootcause::Result<PurchaseOrderDetail> {
    let mut conn = pg_pool.acquire().await?;

    let row = sqlx::query!(
        r#"SELECT id, code, supplier_id, status, order_date,
                  expected_delivery_date, currency, total_amount,
                  payment_terms, remark, created_by
           FROM purchase_orders WHERE id = $1"#,
        &*path.id
    )
    .fetch_optional(&mut *conn)
    .await?
    .ok_or(PurchaseError::NotFound)?;

    let order = PurchaseOrder {
        id: ID::new_unchecked(row.id),
        code: row.code,
        supplier_id: ID::new_unchecked(row.supplier_id),
        status: row.status,
        order_date: row.order_date,
        expected_delivery_date: row.expected_delivery_date,
        currency: row.currency,
        total_amount: row.total_amount,
        payment_terms: row.payment_terms,
        remark: row.remark,
        created_by: row.created_by.map(ID::new_unchecked),
    };

    let line_rows = sqlx::query!(
        r#"SELECT id, order_id, line_no, item_id, quantity, unit,
                  unit_price, line_total, received_qty, returned_qty,
                  closed, remark
           FROM purchase_order_lines WHERE order_id = $1 ORDER BY line_no"#,
        &*path.id
    )
    .fetch_all(&mut *conn)
    .await?;

    let lines = line_rows
        .into_iter()
        .map(|r| PurchaseOrderLine {
            id: ID::new_unchecked(r.id),
            order_id: ID::new_unchecked(r.order_id),
            line_no: r.line_no,
            item_id: ID::new_unchecked(r.item_id),
            quantity: r.quantity,
            unit: r.unit,
            unit_price: r.unit_price,
            line_total: r.line_total,
            received_qty: r.received_qty,
            returned_qty: r.returned_qty,
            closed: r.closed,
            remark: r.remark,
        })
        .collect();

    Ok(PurchaseOrderDetail { order, lines })
}

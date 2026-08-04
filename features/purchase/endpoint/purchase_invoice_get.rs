use axum::extract::State;
use db::PgPool;
use purchase_contract::entity::PurchaseInvoice;
use purchase_contract::error::PurchaseError;
use serde::{Deserialize, Serialize};
use shared_contract::value_object::id::ID;
use utoipa::{IntoParams, ToSchema};
use validify::Validify;
use web::extract::valid_path::ValidPath;
use web::response::json_response::{JsonResponse, JsonResponseType};

#[derive(Debug, Deserialize, Validify, IntoParams)]
#[into_params(parameter_in = Path)]
pub(crate) struct GetInvoicePath {
    pub id: ID,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct InvoiceDetail {
    pub invoice: PurchaseInvoice,
}

#[utoipa::path(get, path = "/api/v1/purchase-invoices/{id}",
    operation_id = "purchase_invoice_get", tag = "purchase-invoice",
    params(GetInvoicePath), responses((status = 200, body = JsonResponse<InvoiceDetail>)),
    security(("bearerAuth" = [])))]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ValidPath(path): ValidPath<GetInvoicePath>,
) -> JsonResponseType<InvoiceDetail> {
    let response = execute(&pg_pool, path).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip_all)]
#[inline]
async fn execute(pg_pool: &PgPool, path: GetInvoicePath) -> rootcause::Result<InvoiceDetail> {
    let mut conn = pg_pool.acquire().await?;
    let row = sqlx::query!(
        r#"SELECT id, code, order_id, supplier_id, invoice_number, invoice_date,
                  amount, tax_amount, total_amount, status, remark
           FROM purchase_invoices WHERE id = $1"#,
        &*path.id,
    )
    .fetch_optional(&mut *conn)
    .await?
    .ok_or(PurchaseError::NotFound)?;
    Ok(InvoiceDetail {
        invoice: PurchaseInvoice {
            id: ID::new_unchecked(row.id),
            code: row.code,
            order_id: ID::new_unchecked(row.order_id),
            supplier_id: ID::new_unchecked(row.supplier_id),
            invoice_number: row.invoice_number,
            invoice_date: row.invoice_date,
            amount: row.amount,
            tax_amount: row.tax_amount,
            total_amount: row.total_amount,
            status: row.status,
            remark: row.remark,
        },
    })
}

use audit_contract::AuditService;
use axum::extract::State;
use code_gen::CodeGen;
use db::PgPool;
use http_auth::extract::operator::OperatorContext;
use purchase_contract::entity::PurchaseInvoice;
use purchase_contract::error::PurchaseError;
use serde::{Deserialize, Serialize};
use shared_contract::value_object::id::ID;
use sqlx::Acquire;
use utoipa::ToSchema;
use validify::Validify;
use web::extract::valid_json::ValidJson;
use web::response::json_response::{JsonResponse, JsonResponseType};

#[derive(Debug, Deserialize, Validify, ToSchema)]
pub(crate) struct CreateInvoiceRequest {
    pub order_id: ID,
    pub invoice_number: Option<String>,
    pub invoice_date: Option<chrono::NaiveDate>,
    pub amount: i64,
    pub tax_amount: Option<i64>,
    pub remark: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct CreateInvoiceResponse {
    pub id: ID,
    pub code: String,
}

#[utoipa::path(
    post,
    path = "/api/v1/purchase-invoices",
    operation_id = "purchase_invoice_create",
    tag = "purchase-invoice",
    request_body = CreateInvoiceRequest,
    responses((status = 200, body = JsonResponse<CreateInvoiceResponse>)),
    security(("bearerAuth" = []))
)]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ctx: OperatorContext,
    ValidJson(request): ValidJson<CreateInvoiceRequest>,
) -> JsonResponseType<CreateInvoiceResponse> {
    let response = execute(&pg_pool, ctx, request).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip_all)]
#[inline]
async fn execute(
    pg_pool: &PgPool,
    ctx: OperatorContext,
    request: CreateInvoiceRequest,
) -> rootcause::Result<CreateInvoiceResponse> {
    let mut conn = pg_pool.acquire().await?;
    let mut txn = conn.begin().await?;

    // 验证订单存在
    let order = sqlx::query!(
        r#"SELECT supplier_id FROM purchase_orders WHERE id = $1"#,
        &*request.order_id
    )
    .fetch_optional(&mut *txn)
    .await?
    .ok_or(PurchaseError::NotFound)?;

    let code = CodeGen::next_code(&mut txn, "seq_purchase_invoice", "INV").await?;

    let id = ID::new();
    let total = request.amount + request.tax_amount.unwrap_or(0);
    // 开票日期缺省为当天（否则发票进不了账龄/报表口径）
    let invoice_date = request
        .invoice_date
        .unwrap_or_else(shared_contract::value_object::today::today_naive);
    sqlx::query!(
        r#"INSERT INTO purchase_invoices
               (id, code, order_id, supplier_id, invoice_number, invoice_date,
                amount, tax_amount, total_amount, remark)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)"#,
        &*id,
        code,
        &*request.order_id,
        &*ID::new_unchecked(order.supplier_id),
        request.invoice_number,
        invoice_date,
        request.amount,
        request.tax_amount.unwrap_or(0),
        total,
        request.remark,
    )
    .execute(&mut *txn)
    .await?;

    // 变更历史：同事务读回写入后的发票作为快照
    let invoice = sqlx::query_as!(
        PurchaseInvoice,
        r#"SELECT id, code, order_id, supplier_id, invoice_number, invoice_date,
                  amount, tax_amount, total_amount, status, remark
           FROM purchase_invoices WHERE id = $1"#,
        &*id
    )
    .fetch_one(&mut *txn)
    .await?;
    AuditService::record_create(&mut txn, "purchase_invoice", &id, &ctx, &invoice).await?;

    txn.commit().await?;
    Ok(CreateInvoiceResponse { id, code })
}

#[cfg(test)]
mod tests {
    use super::*;
    use appctx::testing;
    use migration::run_migrations;
    use shared_contract::value_object::id::ID;

    #[sqlx::test]
    async fn test_invoice_date_defaults_to_today(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let _state = testing::build(pool.clone()).await;
        let mut conn = pool.acquire().await.unwrap();

        let supplier_id = ID::new();
        let po_id = ID::new();
        sqlx::query!(
            "INSERT INTO suppliers (id, code, name, is_active) VALUES ($1, 'S-INVD1', 'Test', true)",
            &*supplier_id
        )
        .execute(&mut *conn)
        .await
        .unwrap();
        sqlx::query!(
            "INSERT INTO purchase_orders (id, code, supplier_id, status, order_date, currency, total_amount) VALUES ($1, 'PO-INVD1', $2, 0, CURRENT_DATE, 'CNY', 100)",
            &*po_id, &*supplier_id
        )
        .execute(&mut *conn)
        .await
        .unwrap();

        let resp = execute(
            &pool,
            crate::tests::test_operator_context(),
            CreateInvoiceRequest {
                order_id: po_id,
                invoice_number: Some("INV-D1".into()),
                invoice_date: None,
                amount: 100,
                tax_amount: None,
                remark: None,
            },
        )
        .await
        .unwrap();

        let mut conn = pool.acquire().await.unwrap();
        let row = sqlx::query!(
            "SELECT invoice_date FROM purchase_invoices WHERE id = $1",
            &*resp.id
        )
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        assert_eq!(
            row.invoice_date
                .expect("invoice_date should default to today"),
            shared_contract::value_object::today::today_naive()
        );

        // 变更历史：create 类型，before 为空
        let audit_row = sqlx::query!(
            r#"SELECT action, before, after FROM audit_logs WHERE entity_id = $1"#,
            *resp.id
        )
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        assert_eq!(audit_row.action, 1); // Created
        assert!(audit_row.before.is_none());
        let after: serde_json::Value = audit_row.after.unwrap();
        assert_eq!(after["total_amount"], 100);
    }
}

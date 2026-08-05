use audit_contract::AuditService;
use axum::extract::State;
use code_gen::CodeGen;
use db::PgPool;
use http_auth::extract::operator::OperatorContext;
use sales_contract::entity::SalesInvoice;
use sales_contract::error::SalesError;
use serde::{Deserialize, Serialize};
use shared_contract::value_object::id::ID;
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
    post, path = "/api/v1/sales-invoices",
    operation_id = "sales_invoice_create", tag = "sales-invoice",
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
    let order = sqlx::query!(
        "SELECT customer_id FROM sales_orders WHERE id = $1",
        &*request.order_id
    )
    .fetch_optional(&mut *conn)
    .await?
    .ok_or(SalesError::NotFound)?;

    let code = CodeGen::next_code(&mut conn, "seq_sales_invoice", "SINV").await?;

    let id = ID::new();
    let total = request.amount + request.tax_amount.unwrap_or(0);
    // 开票日期缺省为当天（否则发票进不了账龄/报表口径）
    let invoice_date = request
        .invoice_date
        .unwrap_or_else(shared_contract::value_object::today::today_naive);
    sqlx::query!(
        r#"INSERT INTO sales_invoices (id, code, order_id, customer_id, invoice_number, invoice_date,
            amount, tax_amount, total_amount, remark) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)"#,
        &*id, code, &*request.order_id, &*ID::new_unchecked(order.customer_id),
        request.invoice_number, invoice_date, request.amount,
        request.tax_amount.unwrap_or(0), total, request.remark,
    ).execute(&mut *conn).await?;

    // 变更历史：本端点无事务（acquire 直连写），同一连接上回读整行并记录创建快照
    let row = sqlx::query!(
        r#"SELECT id, code, order_id, customer_id, invoice_number, invoice_date, amount,
                  tax_amount, total_amount, status, remark
           FROM sales_invoices WHERE id = $1"#,
        &*id
    )
    .fetch_one(&mut *conn)
    .await?;
    let invoice = SalesInvoice {
        id: ID::new_unchecked(row.id),
        code: row.code,
        order_id: ID::new_unchecked(row.order_id),
        customer_id: ID::new_unchecked(row.customer_id),
        invoice_number: row.invoice_number,
        invoice_date: row.invoice_date,
        amount: row.amount,
        tax_amount: row.tax_amount,
        total_amount: row.total_amount,
        status: row.status,
        remark: row.remark,
    };
    AuditService::record_create(&mut conn, "sales_invoice", &id, &ctx, &invoice).await?;

    Ok(CreateInvoiceResponse { id, code })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests;
    use appctx::testing;
    use migration::run_migrations;

    #[sqlx::test]
    async fn test_invoice_create_success(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool.clone()).await;
        let customer_id = tests::insert_test_customer(&state.pg_pool, "C-INV-1").await;
        let order_id =
            tests::insert_test_sales_order(&state.pg_pool, "SO-INV-1", &customer_id, 3).await;

        let req = CreateInvoiceRequest {
            order_id,
            invoice_number: Some("INV-001".into()),
            invoice_date: None,
            amount: 1000,
            tax_amount: Some(100),
            remark: None,
        };
        let resp = execute(&state.pg_pool, tests::test_operator_context(), req)
            .await
            .unwrap();
        assert!(resp.code.starts_with("SINV-"));

        let row = sqlx::query!(
            "SELECT total_amount, customer_id, invoice_date FROM sales_invoices WHERE id = $1",
            &*resp.id
        )
        .fetch_one(&mut *state.pg_pool.acquire().await.unwrap())
        .await
        .unwrap();
        assert_eq!(row.total_amount, 1100);
        assert_eq!(row.customer_id, i64::from(customer_id));
        // 开票日期缺省为当天
        assert_eq!(
            row.invoice_date
                .expect("invoice_date should default to today"),
            shared_contract::value_object::today::today_naive()
        );

        // 变更历史：create 类型，before 为空
        let audit_row = sqlx::query!(
            r#"SELECT action, entity, before, after FROM audit_logs WHERE entity_id = $1"#,
            *resp.id
        )
        .fetch_one(&mut *state.pg_pool.acquire().await.unwrap())
        .await
        .unwrap();
        assert_eq!(audit_row.action, 1); // Created
        assert_eq!(audit_row.entity, "sales_invoice");
        assert!(audit_row.before.is_none());
        let after: serde_json::Value = audit_row.after.unwrap();
        assert_eq!(after["code"], resp.code);
        assert_eq!(after["total_amount"], 1100);
        assert_eq!(after["status"], 0);
    }

    #[sqlx::test]
    async fn test_invoice_order_not_found(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool.clone()).await;

        let req = CreateInvoiceRequest {
            order_id: ID::new(),
            invoice_number: None,
            invoice_date: None,
            amount: 1000,
            tax_amount: None,
            remark: None,
        };
        let err = execute(&state.pg_pool, tests::test_operator_context(), req)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("sales_document_not_found"));
    }
}

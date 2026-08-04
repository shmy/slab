use axum::extract::State;
use code_gen::CodeGen;
use db::PgPool;
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
    ValidJson(request): ValidJson<CreateInvoiceRequest>,
) -> JsonResponseType<CreateInvoiceResponse> {
    let response = execute(&pg_pool, request).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip_all)]
#[inline]
async fn execute(
    pg_pool: &PgPool,
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
        let resp = execute(&state.pg_pool, req).await.unwrap();
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
        let err = execute(&state.pg_pool, req).await.unwrap_err();
        assert!(err.to_string().contains("sales_document_not_found"));
    }
}

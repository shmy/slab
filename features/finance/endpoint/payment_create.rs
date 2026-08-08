use audit_contract::AuditService;
use axum::extract::State;
use db::PgPool;
use doc_numbering::DocNumberer;
use finance_contract::entity::Payment;
use finance_contract::error::FinanceError;
use finance_contract::port::{InvoicePort, InvoiceType};
use http_auth::extract::operator::OperatorContext;
use serde::{Deserialize, Serialize};
use shared_contract::value_object::id::ID;
use sqlx::Connection;
use utoipa::ToSchema;
use validify::Validify;
use web::extract::valid_json::ValidJson;
use web::response::json_response::{JsonResponse, JsonResponseType};

#[derive(Debug, Deserialize, Validify, ToSchema)]
pub(crate) struct CreatePaymentRequest {
    /// 1=AR(收款) 2=AP(付款)
    pub payment_type: i16,
    /// 'sales_invoice' 或 'purchase_invoice'
    pub invoice_type: String,
    pub invoice_id: ID,
    pub amount: i64,
    pub payment_date: Option<chrono::NaiveDate>,
    pub payment_method: Option<String>,
    pub remark: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct CreatePaymentResponse {
    pub id: ID,
    pub code: String,
}

#[utoipa::path(
    post, path = "/api/v1/payments",
    operation_id = "payment_create", tag = "payment",
    request_body = CreatePaymentRequest,
    responses((status = 200, body = JsonResponse<CreatePaymentResponse>)),
    security(("bearerAuth" = []))
)]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ctx: OperatorContext,
    ValidJson(request): ValidJson<CreatePaymentRequest>,
) -> JsonResponseType<CreatePaymentResponse> {
    let response = execute(&pg_pool, ctx, request).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip_all)]
#[inline]
async fn execute(
    pg_pool: &PgPool,
    ctx: OperatorContext,
    request: CreatePaymentRequest,
) -> rootcause::Result<CreatePaymentResponse> {
    if request.amount <= 0 {
        return Err(FinanceError::InvalidPaymentAmount.into());
    }

    let mut conn = pg_pool.acquire().await?;
    let mut txn = conn.begin().await?;

    // Validate and get invoice info
    let invoice_type: InvoiceType = request.invoice_type.parse()?;
    let inv = InvoicePort::by_id(&mut txn, invoice_type, &request.invoice_id).await?;

    let new_paid = inv.paid_amount + request.amount;
    if new_paid > inv.total_amount {
        return Err(FinanceError::InvoiceAlreadyFullyPaid.into());
    }

    // Generate code
    let code = DocNumberer::next_number(&mut txn, "seq_payment", "PAY").await?;

    let id = ID::new();
    let payment_date = request
        .payment_date
        .unwrap_or_else(|| chrono::Utc::now().date_naive());

    // Insert payment
    sqlx::query!(
        r#"INSERT INTO payments (id, code, payment_type, invoice_type, invoice_id, amount, payment_date, payment_method, remark)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)"#,
        &*id, code, request.payment_type, request.invoice_type,
        &*request.invoice_id, request.amount, payment_date,
        request.payment_method, request.remark,
    ).execute(&mut *txn).await?;

    // 变更历史：创建付款记录（同事务写，回滚即消失）
    let payment = sqlx::query_as!(
        Payment,
        r#"SELECT
               id as "id: ID",
               code,
               payment_type,
               invoice_type,
               invoice_id as "invoice_id: ID",
               amount,
               payment_date,
               payment_method,
               remark
           FROM payments
           WHERE id = $1"#,
        &*id
    )
    .fetch_one(&mut *txn)
    .await?;
    AuditService::record_create(&mut txn, "payment", &id, &ctx, &payment).await?;

    // Update invoice paid_amount
    match invoice_type {
        InvoiceType::Sales => {
            sqlx::query!(
                "UPDATE sales_invoices SET paid_amount = $1 WHERE id = $2",
                new_paid,
                &*request.invoice_id,
            )
            .execute(&mut *txn)
            .await?;
        }
        InvoiceType::Purchase => {
            sqlx::query!(
                "UPDATE purchase_invoices SET paid_amount = $1 WHERE id = $2",
                new_paid,
                &*request.invoice_id,
            )
            .execute(&mut *txn)
            .await?;
        }
    }

    txn.commit().await?;
    Ok(CreatePaymentResponse { id, code })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests;
    use appctx::testing;
    use migration::run_migrations;

    #[sqlx::test]
    async fn test_create_sales_payment(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let _state = testing::build(pool.clone()).await;

        // Seed: create a sales invoice
        let customer_id = ID::new();
        let order_id = ID::new();
        let inv_id = ID::new();
        let mut conn = pool.acquire().await.unwrap();

        sqlx::query!(
            "INSERT INTO customers (id, code, name, is_active) VALUES ($1, 'C001', 'Test', true)",
            &*customer_id
        )
        .execute(&mut *conn)
        .await
        .unwrap();
        sqlx::query!("INSERT INTO sales_orders (id, code, customer_id, status, order_date, currency, total_amount) VALUES ($1, 'SO001', $2, 0, CURRENT_DATE, 'CNY', 10000)",
            &*order_id, &*customer_id).execute(&mut *conn).await.unwrap();
        sqlx::query!("INSERT INTO sales_invoices (id, code, order_id, customer_id, amount, tax_amount, total_amount, status) VALUES ($1, 'SINV001', $2, $3, 8000, 2000, 10000, 0)",
            &*inv_id, &*order_id, &*customer_id).execute(&mut *conn).await.unwrap();

        let req = CreatePaymentRequest {
            payment_type: 1, // AR
            invoice_type: "sales_invoice".into(),
            invoice_id: inv_id,
            amount: 5000,
            payment_date: None,
            payment_method: Some("bank_transfer".into()),
            remark: None,
        };

        let resp = execute(&pool, tests::test_operator_context(), req)
            .await
            .unwrap();
        assert!(resp.code.starts_with("PAY-"));
        assert!(resp.id.to_string().len() > 10);

        // Verify paid_amount was updated
        let row = sqlx::query!(
            "SELECT paid_amount FROM sales_invoices WHERE id = $1",
            &*inv_id
        )
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        assert_eq!(row.paid_amount, 5000);

        // 变更历史：create 类型，快照含付款记录字段
        let audit_row = sqlx::query!(
            r#"SELECT entity, action, before, after FROM audit_logs WHERE entity_id = $1"#,
            *resp.id
        )
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        assert_eq!(audit_row.entity, "payment");
        assert_eq!(audit_row.action, 1); // Created
        assert!(audit_row.before.is_none());
        let after: serde_json::Value = audit_row.after.unwrap();
        assert_eq!(after["code"], serde_json::Value::String(resp.code));
        assert_eq!(after["amount"], 5000);
        assert_eq!(after["invoice_type"], "sales_invoice");
    }

    #[sqlx::test]
    async fn test_create_purchase_payment(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let _state = testing::build(pool.clone()).await;

        let supplier_id = ID::new();
        let order_id = ID::new();
        let inv_id = ID::new();
        let mut conn = pool.acquire().await.unwrap();

        sqlx::query!(
            "INSERT INTO suppliers (id, code, name, is_active) VALUES ($1, 'S001', 'Test', true)",
            &*supplier_id
        )
        .execute(&mut *conn)
        .await
        .unwrap();
        sqlx::query!("INSERT INTO purchase_orders (id, code, supplier_id, status, order_date, currency, total_amount) VALUES ($1, 'PO001', $2, 0, CURRENT_DATE, 'CNY', 20000)",
            &*order_id, &*supplier_id).execute(&mut *conn).await.unwrap();
        sqlx::query!("INSERT INTO purchase_invoices (id, code, order_id, supplier_id, amount, tax_amount, total_amount, status) VALUES ($1, 'PINV001', $2, $3, 16000, 4000, 20000, 0)",
            &*inv_id, &*order_id, &*supplier_id).execute(&mut *conn).await.unwrap();

        let req = CreatePaymentRequest {
            payment_type: 2, // AP
            invoice_type: "purchase_invoice".into(),
            invoice_id: inv_id,
            amount: 20000,
            payment_date: None,
            payment_method: Some("bank_transfer".into()),
            remark: Some("full payment".into()),
        };

        let resp = execute(&pool, tests::test_operator_context(), req)
            .await
            .unwrap();
        assert!(resp.code.starts_with("PAY-"));

        let row = sqlx::query!(
            "SELECT paid_amount FROM purchase_invoices WHERE id = $1",
            &*inv_id
        )
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        assert_eq!(row.paid_amount, 20000);

        // 变更历史：create 类型，快照含付款记录字段
        let audit_row = sqlx::query!(
            r#"SELECT entity, action, before, after FROM audit_logs WHERE entity_id = $1"#,
            *resp.id
        )
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        assert_eq!(audit_row.entity, "payment");
        assert_eq!(audit_row.action, 1); // Created
        assert!(audit_row.before.is_none());
        let after: serde_json::Value = audit_row.after.unwrap();
        assert_eq!(after["code"], serde_json::Value::String(resp.code));
        assert_eq!(after["amount"], 20000);
        assert_eq!(after["invoice_type"], "purchase_invoice");
    }

    #[sqlx::test]
    async fn test_payment_exceeds_invoice(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let _state = testing::build(pool.clone()).await;

        let customer_id = ID::new();
        let order_id = ID::new();
        let inv_id = ID::new();
        let mut conn = pool.acquire().await.unwrap();

        sqlx::query!(
            "INSERT INTO customers (id, code, name, is_active) VALUES ($1, 'C002', 'Test', true)",
            &*customer_id
        )
        .execute(&mut *conn)
        .await
        .unwrap();
        sqlx::query!("INSERT INTO sales_orders (id, code, customer_id, status, order_date, currency, total_amount) VALUES ($1, 'SO002', $2, 0, CURRENT_DATE, 'CNY', 5000)",
            &*order_id, &*customer_id).execute(&mut *conn).await.unwrap();
        sqlx::query!("INSERT INTO sales_invoices (id, code, order_id, customer_id, amount, tax_amount, total_amount, status) VALUES ($1, 'SINV002', $2, $3, 4000, 1000, 5000, 0)",
            &*inv_id, &*order_id, &*customer_id).execute(&mut *conn).await.unwrap();

        let req = CreatePaymentRequest {
            payment_type: 1,
            invoice_type: "sales_invoice".into(),
            invoice_id: inv_id,
            amount: 6000,
            payment_date: None,
            payment_method: None,
            remark: None,
        };

        let err = execute(&pool, tests::test_operator_context(), req)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("invoice_already_fully_paid"));
    }

    #[sqlx::test]
    async fn test_invalid_invoice_type(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let _state = testing::build(pool.clone()).await;

        let req = CreatePaymentRequest {
            payment_type: 1,
            invoice_type: "unknown".into(),
            invoice_id: ID::new(),
            amount: 1000,
            payment_date: None,
            payment_method: None,
            remark: None,
        };

        let err = execute(&pool, tests::test_operator_context(), req)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("invalid_invoice_type"));
    }

    #[sqlx::test]
    async fn test_zero_amount_rejected(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let _state = testing::build(pool.clone()).await;

        let req = CreatePaymentRequest {
            payment_type: 1,
            invoice_type: "sales_invoice".into(),
            invoice_id: ID::new(),
            amount: 0,
            payment_date: None,
            payment_method: None,
            remark: None,
        };

        let err = execute(&pool, tests::test_operator_context(), req)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("invalid_payment_amount"));
    }

    #[sqlx::test]
    async fn test_negative_amount_rejected(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let _state = testing::build(pool.clone()).await;

        let req = CreatePaymentRequest {
            payment_type: 1,
            invoice_type: "sales_invoice".into(),
            invoice_id: ID::new(),
            amount: -100,
            payment_date: None,
            payment_method: None,
            remark: None,
        };

        let err = execute(&pool, tests::test_operator_context(), req)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("invalid_payment_amount"));
    }
}

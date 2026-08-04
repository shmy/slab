use axum::extract::State;
use db::PgPool;
use finance_contract::port::{InvoicePort, InvoiceType};
use serde::Serialize;
use utoipa::ToSchema;
use web::response::json_response::{JsonResponse, JsonResponseType};

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct AgingBucket {
    pub bucket: String, // "0-30", "31-60", "61-90", "90+"
    pub count: i64,
    pub amount: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct AgingResponse {
    pub ar: Vec<AgingBucket>,
    pub ap: Vec<AgingBucket>,
}

#[utoipa::path(
    get, path = "/api/v1/finance/aging",
    operation_id = "finance_aging", tag = "finance",
    responses((status = 200, body = JsonResponse<AgingResponse>)),
    security(("bearerAuth" = []))
)]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(State(pg_pool): State<PgPool>) -> JsonResponseType<AgingResponse> {
    let response = execute(&pg_pool).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip_all)]
#[inline]
async fn execute(pg_pool: &PgPool) -> rootcause::Result<AgingResponse> {
    let mut conn = pg_pool.acquire().await?;

    // 账龄桶与未付金额口径由 InvoicePort 统一提供，两端点（aging/balances）共用
    let ar = InvoicePort::unpaid_aging(&mut conn, InvoiceType::Sales).await?;
    let ap = InvoicePort::unpaid_aging(&mut conn, InvoiceType::Purchase).await?;

    let ar = ar
        .into_iter()
        .map(|b| AgingBucket {
            bucket: b.bucket,
            count: b.count,
            amount: b.amount,
        })
        .collect();
    let ap = ap
        .into_iter()
        .map(|b| AgingBucket {
            bucket: b.bucket,
            count: b.count,
            amount: b.amount,
        })
        .collect();

    Ok(AgingResponse { ar, ap })
}

#[cfg(test)]
mod tests {
    use super::*;
    use appctx::testing;
    use migration::run_migrations;
    use shared_contract::value_object::id::ID;

    #[sqlx::test]
    async fn test_aging(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let _state = testing::build(pool.clone()).await;
        let mut conn = pool.acquire().await.unwrap();

        let customer_id = ID::new();
        let order_id1 = ID::new();
        let order_id2 = ID::new();
        let supplier_id = ID::new();
        let po_id = ID::new();

        sqlx::query!(
            "INSERT INTO customers (id, code, name, is_active) VALUES ($1, 'C001', 'Test', true)",
            &*customer_id
        )
        .execute(&mut *conn)
        .await
        .unwrap();
        sqlx::query!("INSERT INTO sales_orders (id, code, customer_id, status, order_date, currency, total_amount) VALUES ($1, 'SO001', $2, 0, CURRENT_DATE, 'CNY', 10000)",
            &*order_id1, &*customer_id).execute(&mut *conn).await.unwrap();
        sqlx::query!("INSERT INTO sales_orders (id, code, customer_id, status, order_date, currency, total_amount) VALUES ($1, 'SO002', $2, 0, CURRENT_DATE - 45, 'CNY', 20000)",
            &*order_id2, &*customer_id).execute(&mut *conn).await.unwrap();

        // Sales invoice 1: current (0-30 days)
        sqlx::query!(
            "INSERT INTO sales_invoices (id, code, order_id, customer_id, amount, tax_amount, total_amount, invoice_date, status) VALUES ($1, 'SINV001', $2, $3, 8000, 2000, 10000, CURRENT_DATE, 0)",
            &*ID::new(), &*order_id1, &*customer_id
        ).execute(&mut *conn).await.unwrap();

        // Sales invoice 2: 45 days old (31-60 bucket), partially paid 5000
        sqlx::query!(
            "INSERT INTO sales_invoices (id, code, order_id, customer_id, amount, tax_amount, total_amount, invoice_date, paid_amount, status) VALUES ($1, 'SINV002', $2, $3, 16000, 4000, 20000, CURRENT_DATE - 45, 5000, 0)",
            &*ID::new(), &*order_id2, &*customer_id
        ).execute(&mut *conn).await.unwrap();

        // Supplier + purchase invoice
        sqlx::query!(
            "INSERT INTO suppliers (id, code, name, is_active) VALUES ($1, 'S001', 'Test', true)",
            &*supplier_id
        )
        .execute(&mut *conn)
        .await
        .unwrap();
        sqlx::query!("INSERT INTO purchase_orders (id, code, supplier_id, status, order_date, currency, total_amount) VALUES ($1, 'PO001', $2, 0, CURRENT_DATE - 100, 'CNY', 30000)",
            &*po_id, &*supplier_id).execute(&mut *conn).await.unwrap();
        sqlx::query!(
            "INSERT INTO purchase_invoices (id, code, order_id, supplier_id, amount, tax_amount, total_amount, invoice_date, status) VALUES ($1, 'PINV001', $2, $3, 24000, 6000, 30000, CURRENT_DATE - 100, 0)",
            &*ID::new(), &*po_id, &*supplier_id
        ).execute(&mut *conn).await.unwrap();

        let result = execute(&pool).await.unwrap();

        // AR: 2 invoices, one in 0-30 (10000 unpaid), one in 31-60 (15000 unpaid)
        assert_eq!(result.ar.len(), 2);
        let bucket_0_30 = result.ar.iter().find(|b| b.bucket == "0-30").unwrap();
        assert_eq!(bucket_0_30.count, 1);
        assert_eq!(bucket_0_30.amount, 10000);
        let bucket_31_60 = result.ar.iter().find(|b| b.bucket == "31-60").unwrap();
        assert_eq!(bucket_31_60.count, 1);
        assert_eq!(bucket_31_60.amount, 15000);

        // AP: 1 invoice in 90+
        assert_eq!(result.ap.len(), 1);
        assert_eq!(result.ap[0].bucket, "90+");
        assert_eq!(result.ap[0].count, 1);
        assert_eq!(result.ap[0].amount, 30000);
    }
}

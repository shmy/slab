use axum::extract::State;
use db::PgPool;
use finance_contract::port::{InvoicePort, InvoiceType};
use serde::Serialize;
use utoipa::ToSchema;
use web::response::json_response::{JsonResponse, JsonResponseType};

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct BalancesResponse {
    pub ar_balance: i64,
    pub ap_balance: i64,
}

#[utoipa::path(
    get, path = "/api/v1/finance/balances",
    operation_id = "finance_balances", tag = "finance",
    responses((status = 200, body = JsonResponse<BalancesResponse>)),
    security(("bearerAuth" = []))
)]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(State(pg_pool): State<PgPool>) -> JsonResponseType<BalancesResponse> {
    let response = execute(&pg_pool).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip_all)]
#[inline]
async fn execute(pg_pool: &PgPool) -> rootcause::Result<BalancesResponse> {
    let mut conn = pg_pool.acquire().await?;

    // 未付总额口径由 InvoicePort 统一提供（含无开票日期的发票，与账龄口径区分）
    let ar = InvoicePort::unpaid_total(&mut conn, InvoiceType::Sales).await?;
    let ap = InvoicePort::unpaid_total(&mut conn, InvoiceType::Purchase).await?;

    Ok(BalancesResponse {
        ar_balance: ar,
        ap_balance: ap,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use appctx::testing;
    use migration::run_migrations;
    use shared_contract::value_object::id::ID;

    #[sqlx::test]
    async fn test_balances(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let _state = testing::build(pool.clone()).await;
        let mut conn = pool.acquire().await.unwrap();

        let cust_id = ID::new();
        let supp_id = ID::new();
        let so_id = ID::new();
        let po_id = ID::new();

        sqlx::query!(
            "INSERT INTO customers (id, code, name, is_active) VALUES ($1, 'C001', 'Test', true)",
            &*cust_id
        )
        .execute(&mut *conn)
        .await
        .unwrap();
        sqlx::query!(
            "INSERT INTO suppliers (id, code, name, is_active) VALUES ($1, 'S001', 'Test', true)",
            &*supp_id
        )
        .execute(&mut *conn)
        .await
        .unwrap();
        sqlx::query!("INSERT INTO sales_orders (id, code, customer_id, status, order_date, currency, total_amount) VALUES ($1, 'SO001', $2, 0, CURRENT_DATE, 'CNY', 0)",
            &*so_id, &*cust_id).execute(&mut *conn).await.unwrap();
        sqlx::query!("INSERT INTO purchase_orders (id, code, supplier_id, status, order_date, currency, total_amount) VALUES ($1, 'PO001', $2, 0, CURRENT_DATE, 'CNY', 0)",
            &*po_id, &*supp_id).execute(&mut *conn).await.unwrap();

        // Sales invoice: 10000 total, 3000 paid = 7000 AR
        sqlx::query!("INSERT INTO sales_invoices (id, code, order_id, customer_id, amount, tax_amount, total_amount, paid_amount, status) VALUES ($1, 'SINV001', $2, $3, 8000, 2000, 10000, 3000, 0)",
            &*ID::new(), &*so_id, &*cust_id).execute(&mut *conn).await.unwrap();
        // Purchase invoice: 5000 total, unpaid = 5000 AP
        sqlx::query!("INSERT INTO purchase_invoices (id, code, order_id, supplier_id, amount, tax_amount, total_amount, status) VALUES ($1, 'PINV001', $2, $3, 4000, 1000, 5000, 0)",
            &*ID::new(), &*po_id, &*supp_id).execute(&mut *conn).await.unwrap();

        let resp = execute(&pool).await.unwrap();
        assert_eq!(resp.ar_balance, 7000);
        assert_eq!(resp.ap_balance, 5000);
    }
}

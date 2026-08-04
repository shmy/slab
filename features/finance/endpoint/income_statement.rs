use axum::extract::State;
use db::PgPool;
use finance_contract::value_object::FiscalPeriod;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use validify::Validify;
use web::extract::valid_query::ValidQuery;
use web::response::json_response::{JsonResponse, JsonResponseType};

#[derive(Debug, Deserialize, Validify, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct IncomeStatementQuery {
    pub year: i32,
    #[param(example = 1)]
    pub month_start: Option<i32>,
    #[param(example = 12)]
    pub month_end: Option<i32>,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct IncomeStatementResponse {
    pub period: String,
    pub total_income: i64,
    pub total_expense: i64,
    pub gross_profit: i64,
    pub sales_invoice_count: i64,
    pub purchase_invoice_count: i64,
}

#[utoipa::path(
    get, path = "/api/v1/finance/income-statement",
    operation_id = "finance_income_statement", tag = "finance",
    params(IncomeStatementQuery),
    responses((status = 200, body = JsonResponse<IncomeStatementResponse>)),
    security(("bearerAuth" = []))
)]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ValidQuery(query): ValidQuery<IncomeStatementQuery>,
) -> JsonResponseType<IncomeStatementResponse> {
    let response = execute(&pg_pool, query).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip_all)]
#[inline]
async fn execute(
    pg_pool: &PgPool,
    query: IncomeStatementQuery,
) -> rootcause::Result<IncomeStatementResponse> {
    let fiscal = FiscalPeriod::try_new(
        query.year,
        query.month_start.map(|m| m as u32),
        query.month_end.map(|m| m as u32),
    )?;
    let start = fiscal.start_date();
    let end = fiscal.end_date();
    let period = fiscal.label();

    let mut conn = pg_pool.acquire().await?;

    let income: i64 = sqlx::query_scalar!(
        r#"SELECT COALESCE(SUM(total_amount), 0)::BIGINT AS "total!"
           FROM sales_invoices WHERE invoice_date >= $1 AND invoice_date <= $2"#,
        start,
        end,
    )
    .fetch_one(&mut *conn)
    .await?;

    let expense: i64 = sqlx::query_scalar!(
        r#"SELECT COALESCE(SUM(total_amount), 0)::BIGINT AS "total!"
           FROM purchase_invoices WHERE invoice_date >= $1 AND invoice_date <= $2"#,
        start,
        end,
    )
    .fetch_one(&mut *conn)
    .await?;

    let sales_count: i64 = sqlx::query_scalar!(
        r#"SELECT COUNT(*)::BIGINT AS "total!"
           FROM sales_invoices WHERE invoice_date >= $1 AND invoice_date <= $2"#,
        start,
        end,
    )
    .fetch_one(&mut *conn)
    .await?;

    let purchase_count: i64 = sqlx::query_scalar!(
        r#"SELECT COUNT(*)::BIGINT AS "total!"
           FROM purchase_invoices WHERE invoice_date >= $1 AND invoice_date <= $2"#,
        start,
        end,
    )
    .fetch_one(&mut *conn)
    .await?;

    Ok(IncomeStatementResponse {
        period,
        total_income: income,
        total_expense: expense,
        gross_profit: income - expense,
        sales_invoice_count: sales_count,
        purchase_invoice_count: purchase_count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use appctx::testing;
    use migration::run_migrations;
    use shared_contract::value_object::id::ID;

    #[sqlx::test]
    async fn test_income_statement_single_month(pool: sqlx::PgPool) {
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
        sqlx::query!("INSERT INTO sales_orders (id, code, customer_id, status, order_date, currency, total_amount) VALUES ($1, 'SO001', $2, 0, '2026-07-01', 'CNY', 0)",
            &*so_id, &*cust_id).execute(&mut *conn).await.unwrap();
        sqlx::query!("INSERT INTO purchase_orders (id, code, supplier_id, status, order_date, currency, total_amount) VALUES ($1, 'PO001', $2, 0, '2026-07-01', 'CNY', 0)",
            &*po_id, &*supp_id).execute(&mut *conn).await.unwrap();

        sqlx::query!("INSERT INTO sales_invoices (id, code, order_id, customer_id, amount, tax_amount, total_amount, invoice_date, status) VALUES ($1, 'SINV001', $2, $3, 10000, 0, 10000, '2026-07-15', 0)",
            &*ID::new(), &*so_id, &*cust_id).execute(&mut *conn).await.unwrap();
        sqlx::query!("INSERT INTO purchase_invoices (id, code, order_id, supplier_id, amount, tax_amount, total_amount, invoice_date, status) VALUES ($1, 'PINV001', $2, $3, 6000, 0, 6000, '2026-07-20', 0)",
            &*ID::new(), &*po_id, &*supp_id).execute(&mut *conn).await.unwrap();

        let resp = execute(
            &pool,
            IncomeStatementQuery {
                year: 2026,
                month_start: Some(7),
                month_end: Some(7),
            },
        )
        .await
        .unwrap();
        assert_eq!(resp.period, "2026-07");
        assert_eq!(resp.total_income, 10000);
        assert_eq!(resp.total_expense, 6000);
        assert_eq!(resp.gross_profit, 4000);
        assert_eq!(resp.sales_invoice_count, 1);
        assert_eq!(resp.purchase_invoice_count, 1);
    }

    #[sqlx::test]
    async fn test_income_statement_empty(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let _state = testing::build(pool.clone()).await;

        let resp = execute(
            &pool,
            IncomeStatementQuery {
                year: 2025,
                month_start: Some(1),
                month_end: Some(1),
            },
        )
        .await
        .unwrap();
        assert_eq!(resp.total_income, 0);
        assert_eq!(resp.total_expense, 0);
        assert_eq!(resp.gross_profit, 0);
    }

    #[sqlx::test]
    async fn test_invalid_period_rejected(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let _state = testing::build(pool.clone()).await;

        // 结束月早于起始月 → 非法期间，而不是静默返回空报表
        let err = execute(
            &pool,
            IncomeStatementQuery {
                year: 2026,
                month_start: Some(7),
                month_end: Some(3),
            },
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("invalid_period"));
    }
}

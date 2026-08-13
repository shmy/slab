use axum::extract::State;
use db::PgPool;
use sea_query::extension::postgres::PgExpr;
use sea_query::{Expr, ExprTrait as _, Order, PostgresQueryBuilder, Query};
use sea_query_sqlx::SqlxBinder as _;
use serde::{Deserialize, Serialize};
use serde_with::{NoneAsEmptyString, serde_as};
use shared_contract::query::cursor_page::finalize_cursor_page;
use shared_contract::query::paging_query::CursorPagingQuery;
use shared_contract::query::paging_result::CursorPagingResult;
use shared_contract::value_object::id::ID;
use sqlx::FromRow;
use utoipa::{IntoParams, ToSchema};
use validify::Validify;
use web::extract::valid_query::ValidQuery;
use web::response::json_response::{JsonResponse, JsonResponseType};

#[serde_as]
#[derive(Debug, Deserialize, Validify, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct SearchPaymentQuery {
    #[serde(flatten)]
    #[param(inline)]
    pub paging: CursorPagingQuery,
    /// 1=AR(收款) 2=AP(付款)
    pub payment_type: Option<i16>,
    /// 'sales_invoice' 或 'purchase_invoice'
    #[serde_as(as = "NoneAsEmptyString")]
    #[serde(default)]
    pub invoice_type: Option<String>,
    #[serde_as(as = "NoneAsEmptyString")]
    #[serde(default)]
    pub q: Option<String>,
}

#[derive(Debug, Serialize, FromRow, ToSchema)]
pub(crate) struct PaymentItem {
    pub id: ID,
    pub code: String,
    pub payment_type: i16,
    pub invoice_type: String,
    pub invoice_id: ID,
    pub amount: i64,
    pub payment_date: chrono::NaiveDate,
    pub payment_method: Option<String>,
    pub remark: Option<String>,
}

#[utoipa::path(
    get, path = "/api/v1/payments",
    operation_id = "payment_search", tag = "payment",
    params(SearchPaymentQuery),
    responses((status = 200, body = JsonResponse<CursorPagingResult<PaymentItem>>)),
    security(("bearerAuth" = []))
)]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ValidQuery(query): ValidQuery<SearchPaymentQuery>,
) -> JsonResponseType<CursorPagingResult<PaymentItem>> {
    let response = execute(&pg_pool, query).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip_all)]
#[inline]
async fn execute(
    pg_pool: &PgPool,
    query: SearchPaymentQuery,
) -> rootcause::Result<CursorPagingResult<PaymentItem>> {
    let q = query.q.filter(|s| !s.is_empty());
    let page_limit = query.paging.limit();
    let fetch_limit = page_limit + 1;

    let (sql, values) = Query::select()
        .from("payments")
        .columns([
            "id",
            "code",
            "payment_type",
            "invoice_type",
            "invoice_id",
            "amount",
            "payment_date",
            "payment_method",
            "remark",
        ])
        .and_where_option(query.payment_type.map(|t| Expr::col("payment_type").eq(t)))
        .and_where_option(
            query
                .invoice_type
                .filter(|s| !s.is_empty())
                .map(|t| Expr::col("invoice_type").eq(t)),
        )
        .and_where_option(q.map(|q| {
            Expr::col("code")
                .ilike(format!("%{q}%"))
                .or(Expr::col("remark").ilike(format!("%{q}%")))
        }))
        .and_where_option(query.paging.next_cursor_id().map(|c| Expr::col("id").lt(c)))
        .order_by("id", Order::Desc)
        .limit(fetch_limit)
        .build_sqlx(PostgresQueryBuilder);

    let mut conn = pg_pool.acquire().await?;
    let items: Vec<PaymentItem> = sqlx::query_as_with(sqlx::AssertSqlSafe(sql), values)
        .fetch_all(&mut *conn)
        .await?;

    Ok(finalize_cursor_page(items, page_limit, |item| item.id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use appctx::testing;
    use migration::run_migrations;

    #[sqlx::test]
    async fn test_search_payments(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let _state = testing::build(pool.clone()).await;
        let mut conn = pool.acquire().await.unwrap();

        let inv_id = ID::new();
        // Insert two payments
        sqlx::query!(
            r#"INSERT INTO payments (id, code, payment_type, invoice_type, invoice_id, amount, payment_date, payment_method)
               VALUES ($1, 'PAY-20250101-000001', 1, 'sales_invoice', $2, 5000, '2025-01-01', 'bank')"#,
            &*ID::new(), &*inv_id
        ).execute(&mut *conn).await.unwrap();

        let inv2_id = ID::new();
        sqlx::query!(
            r#"INSERT INTO payments (id, code, payment_type, invoice_type, invoice_id, amount, payment_date, payment_method)
               VALUES ($1, 'PAY-20250102-000002', 2, 'purchase_invoice', $2, 10000, '2025-01-02', 'cash')"#,
            &*ID::new(), &*inv2_id
        ).execute(&mut *conn).await.unwrap();

        // Search all
        let result = execute(
            &pool,
            SearchPaymentQuery {
                paging: CursorPagingQuery::default(),
                payment_type: None,
                invoice_type: None,
                q: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(result.items.len(), 2);

        // Filter by type
        let result = execute(
            &pool,
            SearchPaymentQuery {
                paging: CursorPagingQuery::default(),
                payment_type: Some(1),
                invoice_type: None,
                q: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(result.items.len(), 1);
        assert_eq!(result.items[0].payment_type, 1);

        // Filter by invoice_type
        let result = execute(
            &pool,
            SearchPaymentQuery {
                paging: CursorPagingQuery::default(),
                payment_type: None,
                invoice_type: Some("purchase_invoice".into()),
                q: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(result.items.len(), 1);
        assert_eq!(result.items[0].invoice_type, "purchase_invoice");
    }
}

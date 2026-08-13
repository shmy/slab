use axum::extract::State;
use db::PgPool;
use sea_query::{Expr, ExprTrait as _, Order, PostgresQueryBuilder, Query};
use sea_query_sqlx::SqlxBinder as _;
use serde::{Deserialize, Serialize};
use shared_contract::query::cursor_page::finalize_cursor_page;
use shared_contract::query::paging_query::CursorPagingQuery;
use shared_contract::query::paging_result::CursorPagingResult;
use shared_contract::value_object::id::ID;
use sqlx::FromRow;
use utoipa::{IntoParams, ToSchema};
use validify::Validify;
use web::extract::valid_query::ValidQuery;
use web::response::json_response::{JsonResponse, JsonResponseType};

#[derive(Debug, Deserialize, Validify, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct TransactionQuery {
    #[serde(flatten)]
    #[param(inline)]
    pub paging: CursorPagingQuery,
    pub item_id: Option<i64>,
    pub warehouse_id: Option<i64>,
    pub transaction_type: Option<i16>,
}

#[derive(Debug, Serialize, FromRow, ToSchema)]
pub(crate) struct TransactionItem {
    pub id: ID,
    pub item_id: ID,
    pub warehouse_id: ID,
    pub transaction_type: i16,
    pub quantity: i64,
    pub batch_number: Option<String>,
    pub reference_type: String,
    pub reference_id: i64,
    pub before_quantity: i64,
    pub after_quantity: i64,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[utoipa::path(
    get, path = "/api/v1/inventory-transactions",
    operation_id = "inventory_transaction_search", tag = "inventory-transaction",
    params(TransactionQuery),
    responses((status = 200, body = JsonResponse<CursorPagingResult<TransactionItem>>)),
    security(("bearerAuth" = []))
)]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ValidQuery(query): ValidQuery<TransactionQuery>,
) -> JsonResponseType<CursorPagingResult<TransactionItem>> {
    let response = execute(&pg_pool, query).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip_all)]
#[inline]
async fn execute(
    pg_pool: &PgPool,
    query: TransactionQuery,
) -> rootcause::Result<CursorPagingResult<TransactionItem>> {
    let page_limit = query.paging.limit();
    let fetch_limit = page_limit + 1;

    let (sql, values) = Query::select()
        .from("inventory_transactions")
        .column("id")
        .column("item_id")
        .column("warehouse_id")
        .column("transaction_type")
        .column("quantity")
        .column("batch_number")
        .column("reference_type")
        .column("reference_id")
        .column("before_quantity")
        .column("after_quantity")
        .column("created_at")
        .and_where_option(query.item_id.map(|v| Expr::col("item_id").eq(v)))
        .and_where_option(query.warehouse_id.map(|v| Expr::col("warehouse_id").eq(v)))
        .and_where_option(
            query
                .transaction_type
                .map(|v| Expr::col("transaction_type").eq(v)),
        )
        .and_where_option(query.paging.cursor_id().map(|c| Expr::col("id").lt(c)))
        .order_by("id", Order::Desc)
        .limit(fetch_limit)
        .build_sqlx(PostgresQueryBuilder);

    let mut conn = pg_pool.acquire().await?;
    let items: Vec<TransactionItem> = sqlx::query_as_with(sqlx::AssertSqlSafe(sql), values)
        .fetch_all(&mut *conn)
        .await?;
    Ok(finalize_cursor_page(items, page_limit, |t| t.id))
}

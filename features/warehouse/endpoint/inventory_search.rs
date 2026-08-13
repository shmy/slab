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
pub(crate) struct SearchInventoryQuery {
    #[serde(flatten)]
    #[param(inline)]
    pub paging: CursorPagingQuery,
    pub warehouse_id: Option<i64>,
}

#[derive(Debug, Serialize, FromRow, ToSchema)]
pub(crate) struct InventoryItem {
    pub id: ID,
    pub item_id: ID,
    pub warehouse_id: ID,
    pub quantity: f64,
    pub locked_qty: f64,
}

#[utoipa::path(
    get, path = "/api/v1/inventories",
    operation_id = "inventory_search", tag = "inventory",
    params(SearchInventoryQuery),
    responses((status = 200, body = JsonResponse<CursorPagingResult<InventoryItem>>)),
    security(("bearerAuth" = []))
)]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ValidQuery(query): ValidQuery<SearchInventoryQuery>,
) -> JsonResponseType<CursorPagingResult<InventoryItem>> {
    let response = execute(&pg_pool, query).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip_all)]
#[inline]
async fn execute(
    pg_pool: &PgPool,
    query: SearchInventoryQuery,
) -> rootcause::Result<CursorPagingResult<InventoryItem>> {
    let page_limit = query.paging.limit();
    let fetch_limit = page_limit + 1;

    let (sql, values) = Query::select()
        .from("inventories")
        .column("id")
        .column("item_id")
        .column("warehouse_id")
        .expr(Expr::cust("CAST(quantity AS DOUBLE PRECISION)"))
        .expr(Expr::cust("CAST(locked_qty AS DOUBLE PRECISION)"))
        .and_where_option(query.warehouse_id.map(|w| Expr::col("warehouse_id").eq(w)))
        .and_where_option(query.paging.next_cursor_id().map(|c| Expr::col("id").lt(c)))
        .order_by("id", Order::Desc)
        .limit(fetch_limit)
        .build_sqlx(PostgresQueryBuilder);
    let mut conn = pg_pool.acquire().await?;
    let items: Vec<InventoryItem> = sqlx::query_as_with(sqlx::AssertSqlSafe(sql), values)
        .fetch_all(&mut *conn)
        .await?;
    Ok(finalize_cursor_page(items, page_limit, |item| item.id))
}

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
pub(crate) struct SearchItemQuery {
    #[serde(flatten)]
    #[param(inline)]
    pub paging: CursorPagingQuery,
    #[serde_as(as = "NoneAsEmptyString")]
    #[serde(default)]
    pub q: Option<String>,
    pub item_type: Option<i16>,
}

#[derive(Debug, Serialize, FromRow, ToSchema)]
pub(crate) struct SearchItemItem {
    pub id: ID,
    pub code: String,
    pub name: String,
    pub item_type: i16,
    pub is_active: bool,
}

#[utoipa::path(
    get,
    path = "/api/v1/items",
    operation_id = "item_search",
    tag = "item",
    params(SearchItemQuery),
    responses((status = 200, body = JsonResponse<CursorPagingResult<SearchItemItem>>)),
    security(("bearerAuth" = []))
)]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ValidQuery(query): ValidQuery<SearchItemQuery>,
) -> JsonResponseType<CursorPagingResult<SearchItemItem>> {
    let response = execute(&pg_pool, query).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip_all)]
#[inline]
async fn execute(
    pg_pool: &PgPool,
    query: SearchItemQuery,
) -> rootcause::Result<CursorPagingResult<SearchItemItem>> {
    let q = query.q.filter(|s| !s.is_empty());
    let input_next_cursor = query.paging.next_cursor();
    let page_limit = query.paging.limit();
    let fetch_limit = page_limit + 1;

    let (sql, values) = Query::select()
        .from("items")
        .column("id")
        .column("code")
        .column("name")
        .column("item_type")
        .column("is_active")
        .and_where_option(q.map(|q| {
            Expr::col("code")
                .ilike(format!("%{q}%"))
                .or(Expr::col("name").ilike(format!("%{q}%")))
        }))
        .and_where_option(query.item_type.map(|t| Expr::col("item_type").eq(t)))
        .and_where_option(input_next_cursor.map(|next_cursor| Expr::col("id").lt(next_cursor)))
        // 软删除（delete 置 is_active=false）不出现在列表
        .and_where(Expr::col("is_active").eq(true))
        .order_by("id", Order::Desc)
        .limit(fetch_limit)
        .build_sqlx(PostgresQueryBuilder);

    let mut conn = pg_pool.acquire().await?;
    let items: Vec<SearchItemItem> = sqlx::query_as_with(sqlx::AssertSqlSafe(sql), values)
        .fetch_all(&mut *conn)
        .await?;
    Ok(finalize_cursor_page(items, page_limit, |item| item.id))
}

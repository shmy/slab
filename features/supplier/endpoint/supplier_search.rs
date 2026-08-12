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
pub(crate) struct SearchSupplierQuery {
    #[serde(flatten)]
    #[param(inline)]
    pub paging: CursorPagingQuery,
    #[serde_as(as = "NoneAsEmptyString")]
    #[serde(default)]
    pub q: Option<String>,
}

#[derive(Debug, Serialize, FromRow, ToSchema)]
pub(crate) struct SearchSupplierItem {
    pub id: ID,
    pub code: String,
    pub name: String,
    pub is_active: bool,
}

#[utoipa::path(
    get, path = "/api/v1/suppliers", operation_id = "supplier_search", tag = "supplier",
    params(SearchSupplierQuery),
    responses((status = 200, body = JsonResponse<CursorPagingResult<SearchSupplierItem>>)),
    security(("bearerAuth" = []))
)]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ValidQuery(query): ValidQuery<SearchSupplierQuery>,
) -> JsonResponseType<CursorPagingResult<SearchSupplierItem>> {
    let response = execute(&pg_pool, query).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip_all)]
#[inline]
async fn execute(
    pg_pool: &PgPool,
    query: SearchSupplierQuery,
) -> rootcause::Result<CursorPagingResult<SearchSupplierItem>> {
    let q = query.q.filter(|s| !s.is_empty());
    let page_limit = query.paging.limit();
    let fetch_limit = page_limit + 1;
    let (sql, values) = Query::select()
        .from("suppliers")
        .column("id")
        .column("code")
        .column("name")
        .column("is_active")
        .and_where_option(q.map(|q| {
            Expr::col("code")
                .ilike(format!("%{q}%"))
                .or(Expr::col("name").ilike(format!("%{q}%")))
        }))
        .and_where_option(query.paging.next_cursor().map(|c| Expr::col("id").lt(c)))
        // 软删除（delete 置 is_active=false）不出现在列表
        .and_where(Expr::col("is_active").eq(true))
        .order_by("id", Order::Desc)
        .limit(fetch_limit)
        .build_sqlx(PostgresQueryBuilder);
    let mut conn = pg_pool.acquire().await?;
    let items: Vec<SearchSupplierItem> = sqlx::query_as_with(sqlx::AssertSqlSafe(sql), values)
        .fetch_all(&mut *conn)
        .await?;
    Ok(finalize_cursor_page(items, page_limit, |item| item.id))
}

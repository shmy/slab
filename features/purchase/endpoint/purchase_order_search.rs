use axum::extract::State;
use db::PgPool;
use sea_query::{Expr, ExprTrait as _, Query};
use serde::{Deserialize, Serialize};
use shared_contract::query::cursor_page::paginate;
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
pub(crate) struct SearchPurchaseOrderQuery {
    #[serde(flatten)]
    #[param(inline)]
    pub paging: CursorPagingQuery,
    pub supplier_id: Option<i64>,
    pub status: Option<i16>,
}

#[derive(Debug, Serialize, FromRow, ToSchema)]
pub(crate) struct PurchaseOrderItem {
    pub id: ID,
    pub code: String,
    pub supplier_id: ID,
    pub status: i16,
    pub order_date: chrono::NaiveDate,
    pub total_amount: i64,
}

#[utoipa::path(
    get,
    path = "/api/v1/purchase-orders",
    operation_id = "purchase_order_search",
    tag = "purchase-order",
    params(SearchPurchaseOrderQuery),
    responses((status = 200, body = JsonResponse<CursorPagingResult<PurchaseOrderItem>>)),
    security(("bearerAuth" = []))
)]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ValidQuery(query): ValidQuery<SearchPurchaseOrderQuery>,
) -> JsonResponseType<CursorPagingResult<PurchaseOrderItem>> {
    let response = execute(&pg_pool, query).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip_all)]
#[inline]
async fn execute(
    pg_pool: &PgPool,
    query: SearchPurchaseOrderQuery,
) -> rootcause::Result<CursorPagingResult<PurchaseOrderItem>> {
    let select = Query::select()
        .from("purchase_orders")
        .column("id")
        .column("code")
        .column("supplier_id")
        .column("status")
        .column("order_date")
        .column("total_amount")
        .and_where_option(query.supplier_id.map(|s| Expr::col("supplier_id").eq(s)))
        .and_where_option(query.status.map(|s| Expr::col("status").eq(s)))
        .to_owned();

    let mut conn = pg_pool.acquire().await?;
    paginate(&mut conn, select, &query.paging, "id").await
}

use axum::extract::State;
use db::PgPool;
use serde::{Deserialize, Serialize};
use shared_contract::value_object::id::ID;
use utoipa::{IntoParams, ToSchema};
use validify::Validify;
use web::extract::valid_path::ValidPath;
use web::response::json_response::{JsonResponse, JsonResponseType};

#[derive(Debug, Deserialize, Validify, IntoParams)]
#[into_params(parameter_in = Path)]
pub(crate) struct ListCostPath {
    pub item_id: ID,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct CostItem {
    pub id: ID,
    pub cost_type: i16,
    pub unit_cost: i64,
    pub currency: String,
    pub is_current: bool,
    pub remark: Option<String>,
}

#[utoipa::path(
    get,
    path = "/api/v1/items/{item_id}/costs",
    operation_id = "item_cost_list",
    tag = "item-cost",
    params(ListCostPath),
    responses((status = 200, body = JsonResponse<Vec<CostItem>>)),
    security(("bearerAuth" = []))
)]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ValidPath(path): ValidPath<ListCostPath>,
) -> JsonResponseType<Vec<CostItem>> {
    let response = execute(&pg_pool, path).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip(pg_pool))]
#[inline]
async fn execute(pg_pool: &PgPool, path: ListCostPath) -> rootcause::Result<Vec<CostItem>> {
    let mut conn = pg_pool.acquire().await?;
    let rows = sqlx::query!(
        r#"SELECT id, cost_type, unit_cost, currency, is_current, remark
           FROM item_costs WHERE item_id = $1 ORDER BY created_at DESC"#,
        &*path.item_id
    )
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| CostItem {
            id: ID::new_unchecked(r.id),
            cost_type: r.cost_type,
            unit_cost: r.unit_cost,
            currency: r.currency.unwrap_or("CNY".into()),
            is_current: r.is_current,
            remark: r.remark,
        })
        .collect())
}

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
pub(crate) struct ListUnitPath {
    pub item_id: ID,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct UnitItem {
    pub id: ID,
    pub unit: String,
    pub rate: i64,
}

#[utoipa::path(
    get,
    path = "/api/v1/items/{item_id}/units",
    operation_id = "item_unit_list",
    tag = "item-unit",
    params(ListUnitPath),
    responses((status = 200, body = JsonResponse<Vec<UnitItem>>)),
    security(("bearerAuth" = []))
)]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ValidPath(path): ValidPath<ListUnitPath>,
) -> JsonResponseType<Vec<UnitItem>> {
    let response = execute(&pg_pool, path).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip(pg_pool))]
#[inline]
async fn execute(pg_pool: &PgPool, path: ListUnitPath) -> rootcause::Result<Vec<UnitItem>> {
    let mut conn = pg_pool.acquire().await?;
    let rows = sqlx::query!(
        r#"SELECT id, unit, rate FROM item_units WHERE item_id = $1"#,
        &*path.item_id
    )
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| UnitItem {
            id: ID::new_unchecked(r.id),
            unit: r.unit,
            rate: r.rate,
        })
        .collect())
}

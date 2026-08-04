use axum::extract::State;
use db::PgPool;
use item_contract::entity::ItemType;
use item_contract::port::ItemPort;
use serde::{Deserialize, Serialize};
use shared_contract::value_object::id::ID;
use utoipa::{IntoParams, ToSchema};
use validify::Validify;
use web::extract::valid_path::ValidPath;
use web::response::json_response::{JsonResponse, JsonResponseType};

#[derive(Debug, Deserialize, Validify, IntoParams)]
#[into_params(parameter_in = Path)]
pub(crate) struct GetItemPath {
    pub id: ID,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct GetItemResponse {
    pub id: ID,
    pub code: String,
    pub name: String,
    pub category_id: ID,
    pub item_type: ItemType,
    pub base_unit: String,
    pub parent_item_id: Option<ID>,
    pub spec: Option<String>,
    pub is_active: bool,
    pub version: i64,
}

#[utoipa::path(
    get,
    path = "/api/v1/items/{id}",
    operation_id = "item_get",
    tag = "item",
    params(GetItemPath),
    responses((status = 200, body = JsonResponse<GetItemResponse>)),
    security(("bearerAuth" = []))
)]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ValidPath(path): ValidPath<GetItemPath>,
) -> JsonResponseType<GetItemResponse> {
    let response = execute(&pg_pool, path).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip(pg_pool))]
#[inline]
async fn execute(pg_pool: &PgPool, path: GetItemPath) -> rootcause::Result<GetItemResponse> {
    let mut conn = pg_pool.acquire().await?;
    let item = ItemPort::by_id(&mut conn, &path.id)
        .await?
        .ok_or(item_contract::error::ItemError::NotFound)?;
    Ok(GetItemResponse {
        id: item.id,
        code: item.code,
        name: item.name,
        category_id: item.category_id,
        item_type: item.item_type,
        base_unit: item.base_unit,
        parent_item_id: item.parent_item_id,
        spec: item.spec,
        is_active: item.is_active,
        version: item.version,
    })
}

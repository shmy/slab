use axum::extract::State;
use db::PgPool;
use serde::{Deserialize, Serialize};
use shared_contract::value_object::id::ID;
use sqlx::Acquire;
use utoipa::{IntoParams, ToSchema};
use validify::Validify;
use web::extract::{valid_json::ValidJson, valid_path::ValidPath};
use web::response::json_response::{JsonResponse, JsonResponseType};

use crate::repository::item_repository::ItemRepository;

#[derive(Debug, Deserialize, Validify, IntoParams)]
#[into_params(parameter_in = Path)]
pub(crate) struct UpdateItemPath {
    pub id: ID,
}

#[derive(Debug, Deserialize, Validify, ToSchema)]
pub(crate) struct UpdateItemRequest {
    pub name: Option<String>,
    pub category_id: Option<ID>,
    pub base_unit: Option<String>,
    pub parent_item_id: Option<Option<ID>>,
    pub spec: Option<Option<String>>,
    pub is_active: Option<bool>,
    pub reorder_point: Option<i64>,
    pub safety_stock: Option<i64>,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct UpdateItemResponse {
    pub updated: bool,
}

#[utoipa::path(
    patch,
    path = "/api/v1/items/{id}",
    operation_id = "item_update",
    tag = "item",
    params(UpdateItemPath),
    request_body = UpdateItemRequest,
    responses((status = 200, body = JsonResponse<UpdateItemResponse>)),
    security(("bearerAuth" = []))
)]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ValidPath(path): ValidPath<UpdateItemPath>,
    ValidJson(request): ValidJson<UpdateItemRequest>,
) -> JsonResponseType<UpdateItemResponse> {
    let response = execute(&pg_pool, path, request).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip_all)]
#[inline]
async fn execute(
    pg_pool: &PgPool,
    path: UpdateItemPath,
    request: UpdateItemRequest,
) -> rootcause::Result<UpdateItemResponse> {
    let mut conn = pg_pool.acquire().await?;
    let mut txn = conn.begin().await?;
    let updated = ItemRepository::update(&mut txn, &path.id, &request).await?;
    txn.commit().await?;
    Ok(UpdateItemResponse { updated })
}

use axum::extract::State;
use db::PgPool;
use serde::{Deserialize, Serialize};
use shared_contract::value_object::id::ID;
use sqlx::Acquire;
use utoipa::{IntoParams, ToSchema};
use validify::Validify;
use web::extract::{valid_json::ValidJson, valid_path::ValidPath};
use web::response::json_response::{JsonResponse, JsonResponseType};

use crate::repository::item_category_repository::ItemCategoryRepository;

#[derive(Debug, Deserialize, Validify, IntoParams)]
#[into_params(parameter_in = Path)]
pub(crate) struct UpdateCategoryPath {
    pub id: ID,
}

#[derive(Debug, Deserialize, Validify, ToSchema)]
pub(crate) struct UpdateCategoryRequest {
    pub name: Option<String>,
    pub parent_id: Option<Option<ID>>,
    pub sort_order: Option<i32>,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct UpdateCategoryResponse {
    pub updated: bool,
}

#[utoipa::path(
    patch,
    path = "/api/v1/item-categories/{id}",
    operation_id = "item_category_update",
    tag = "item-category",
    params(UpdateCategoryPath),
    request_body = UpdateCategoryRequest,
    responses((status = 200, body = JsonResponse<UpdateCategoryResponse>)),
    security(("bearerAuth" = []))
)]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ValidPath(path): ValidPath<UpdateCategoryPath>,
    ValidJson(request): ValidJson<UpdateCategoryRequest>,
) -> JsonResponseType<UpdateCategoryResponse> {
    let response = execute(&pg_pool, path, request).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip_all)]
#[inline]
async fn execute(
    pg_pool: &PgPool,
    path: UpdateCategoryPath,
    request: UpdateCategoryRequest,
) -> rootcause::Result<UpdateCategoryResponse> {
    let mut conn = pg_pool.acquire().await?;
    let mut txn = conn.begin().await?;
    let updated = ItemCategoryRepository::update(&mut txn, &path.id, &request).await?;
    txn.commit().await?;
    Ok(UpdateCategoryResponse { updated })
}

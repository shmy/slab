use axum::extract::State;
use db::PgPool;
use item_contract::entity::ItemCategory;
use serde::{Deserialize, Serialize};
use shared_contract::value_object::id::ID;
use sqlx::Acquire;
use utoipa::ToSchema;
use validify::Validify;
use web::extract::valid_json::ValidJson;
use web::response::json_response::{JsonResponse, JsonResponseType};

use crate::repository::item_category_repository::ItemCategoryRepository;

#[derive(Debug, Deserialize, Validify, ToSchema)]
pub(crate) struct CreateCategoryRequest {
    pub name: String,
    pub parent_id: Option<ID>,
    pub sort_order: Option<i32>,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct CreateCategoryResponse {
    pub id: ID,
}

#[utoipa::path(
    post,
    path = "/api/v1/item-categories",
    operation_id = "item_category_create",
    tag = "item-category",
    request_body = CreateCategoryRequest,
    responses((status = 200, body = JsonResponse<CreateCategoryResponse>)),
    security(("bearerAuth" = []))
)]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ValidJson(request): ValidJson<CreateCategoryRequest>,
) -> JsonResponseType<CreateCategoryResponse> {
    let response = execute(&pg_pool, request).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip(pg_pool))]
#[inline]
async fn execute(
    pg_pool: &PgPool,
    request: CreateCategoryRequest,
) -> rootcause::Result<CreateCategoryResponse> {
    let id = ID::new();
    let category = ItemCategory {
        id,
        name: request.name,
        parent_id: request.parent_id,
        sort_order: request.sort_order.unwrap_or(0),
        is_active: true,
    };
    let mut conn = pg_pool.acquire().await?;
    let mut txn = conn.begin().await?;
    ItemCategoryRepository::create(&mut txn, &category).await?;
    txn.commit().await?;
    Ok(CreateCategoryResponse { id })
}

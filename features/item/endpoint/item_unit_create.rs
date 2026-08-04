use axum::extract::State;
use db::PgPool;
use item_contract::entity::ItemUnit;
use serde::{Deserialize, Serialize};
use shared_contract::value_object::id::ID;
use sqlx::Acquire;
use utoipa::{IntoParams, ToSchema};
use validify::Validify;
use web::extract::{valid_json::ValidJson, valid_path::ValidPath};
use web::response::json_response::{JsonResponse, JsonResponseType};

use crate::repository::item_unit_repository::ItemUnitRepository;

#[derive(Debug, Deserialize, Validify, IntoParams)]
#[into_params(parameter_in = Path)]
pub(crate) struct CreateUnitPath {
    pub item_id: ID,
}

#[derive(Debug, Deserialize, Validify, ToSchema)]
pub(crate) struct CreateUnitRequest {
    pub unit: String,
    pub rate: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct CreateUnitResponse {
    pub id: ID,
}

#[utoipa::path(
    post,
    path = "/api/v1/items/{item_id}/units",
    operation_id = "item_unit_create",
    tag = "item-unit",
    params(CreateUnitPath),
    request_body = CreateUnitRequest,
    responses((status = 200, body = JsonResponse<CreateUnitResponse>)),
    security(("bearerAuth" = []))
)]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ValidPath(path): ValidPath<CreateUnitPath>,
    ValidJson(request): ValidJson<CreateUnitRequest>,
) -> JsonResponseType<CreateUnitResponse> {
    let response = execute(&pg_pool, path, request).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip(pg_pool))]
#[inline]
async fn execute(
    pg_pool: &PgPool,
    path: CreateUnitPath,
    request: CreateUnitRequest,
) -> rootcause::Result<CreateUnitResponse> {
    let id = ID::new();
    let unit = ItemUnit {
        id,
        item_id: path.item_id,
        unit: request.unit,
        rate: request.rate,
    };
    let mut conn = pg_pool.acquire().await?;
    let mut txn = conn.begin().await?;
    ItemUnitRepository::create(&mut txn, &unit).await?;
    txn.commit().await?;
    Ok(CreateUnitResponse { id })
}

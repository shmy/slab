use axum::extract::State;
use db::PgPool;
use item_contract::entity::{CostType, ItemCost};
use serde::{Deserialize, Serialize};
use shared_contract::value_object::id::ID;
use sqlx::Acquire;
use utoipa::{IntoParams, ToSchema};
use validify::Validify;
use web::extract::{valid_json::ValidJson, valid_path::ValidPath};
use web::response::json_response::{JsonResponse, JsonResponseType};

use crate::repository::item_cost_repository::ItemCostRepository;

#[derive(Debug, Deserialize, Validify, IntoParams)]
#[into_params(parameter_in = Path)]
pub(crate) struct CreateCostPath {
    pub item_id: ID,
}

#[derive(Debug, Deserialize, Validify, ToSchema)]
pub(crate) struct CreateCostRequest {
    pub cost_type: CostType,
    pub unit_cost: i64,
    pub currency: Option<String>,
    pub is_current: Option<bool>,
    pub remark: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct CreateCostResponse {
    pub id: ID,
}

#[utoipa::path(
    post,
    path = "/api/v1/items/{item_id}/costs",
    operation_id = "item_cost_create",
    tag = "item-cost",
    params(CreateCostPath),
    request_body = CreateCostRequest,
    responses((status = 200, body = JsonResponse<CreateCostResponse>)),
    security(("bearerAuth" = []))
)]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ValidPath(path): ValidPath<CreateCostPath>,
    ValidJson(request): ValidJson<CreateCostRequest>,
) -> JsonResponseType<CreateCostResponse> {
    let response = execute(&pg_pool, path, request).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip_all)]
#[inline]
async fn execute(
    pg_pool: &PgPool,
    path: CreateCostPath,
    request: CreateCostRequest,
) -> rootcause::Result<CreateCostResponse> {
    let id = ID::new();
    let cost = ItemCost {
        id,
        item_id: path.item_id,
        cost_type: request.cost_type,
        unit_cost: request.unit_cost,
        currency: request.currency.unwrap_or("CNY".into()),
        effective_at: chrono::Utc::now(),
        is_current: request.is_current.unwrap_or(true),
        remark: request.remark,
    };
    let mut conn = pg_pool.acquire().await?;
    let mut txn = conn.begin().await?;
    ItemCostRepository::create(&mut txn, &cost).await?;
    txn.commit().await?;
    Ok(CreateCostResponse { id })
}

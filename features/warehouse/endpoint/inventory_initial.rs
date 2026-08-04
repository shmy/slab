use axum::extract::State;
use db::PgPool;
use inventory_ledger::InventoryLedger;
use serde::{Deserialize, Serialize};
use shared_contract::value_object::id::ID;
use sqlx::Acquire;
use utoipa::ToSchema;
use validify::Validify;
use web::extract::valid_json::ValidJson;
use web::response::json_response::{JsonResponse, JsonResponseType};

#[derive(Debug, Deserialize, Validify, ToSchema)]
pub(crate) struct InitializeInventoryRequest {
    pub item_id: ID,
    pub warehouse_id: ID,
    pub quantity: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct InitializeInventoryResponse {
    pub id: ID,
}

#[utoipa::path(
    post, path = "/api/v1/inventories/initial",
    operation_id = "inventory_initial", tag = "inventory",
    request_body = InitializeInventoryRequest,
    responses((status = 200, body = JsonResponse<InitializeInventoryResponse>)),
    security(("bearerAuth" = []))
)]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ValidJson(request): ValidJson<InitializeInventoryRequest>,
) -> JsonResponseType<InitializeInventoryResponse> {
    let response = execute(&pg_pool, request).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip_all)]
#[inline]
async fn execute(
    pg_pool: &PgPool,
    request: InitializeInventoryRequest,
) -> rootcause::Result<InitializeInventoryResponse> {
    let id = ID::new();
    let mut conn = pg_pool.acquire().await?;
    let mut txn = conn.begin().await?;
    InventoryLedger::adjust(
        &mut txn,
        &request.item_id,
        &request.warehouse_id,
        request.quantity,
        "inventory_initial",
        &id,
    )
    .await?;
    txn.commit().await?;
    Ok(InitializeInventoryResponse { id })
}

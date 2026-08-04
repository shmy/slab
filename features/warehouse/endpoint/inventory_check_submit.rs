//! 提交盘点单。

use crate::repository::inventory_check_repository::InventoryCheckRepository;
use axum::extract::State;
use db::PgPool;
use serde::{Deserialize, Serialize};
use shared_contract::value_object::id::ID;
use sqlx::Acquire;
use utoipa::{IntoParams, ToSchema};
use validify::Validify;
use web::extract::valid_path::ValidPath;
use web::response::json_response::{JsonResponse, JsonResponseType};

#[derive(Debug, Deserialize, Validify, IntoParams)]
#[into_params(parameter_in = Path)]
pub(crate) struct CheckActionPath {
    pub id: ID,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct CheckActionResponse {
    pub success: bool,
}

#[utoipa::path(post, path = "/api/v1/inventory-checks/{id}/submit",
    operation_id = "inventory_check_submit", tag = "inventory-check",
    params(CheckActionPath),
    responses((status = 200, body = JsonResponse<CheckActionResponse>)),
    security(("bearerAuth" = [])))]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ValidPath(path): ValidPath<CheckActionPath>,
) -> JsonResponseType<CheckActionResponse> {
    let response = execute(&pg_pool, path).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip_all)]
#[inline]
async fn execute(
    pg_pool: &PgPool,
    path: CheckActionPath,
) -> rootcause::Result<CheckActionResponse> {
    let mut conn = pg_pool.acquire().await?;
    let mut txn = conn.begin().await?;

    InventoryCheckRepository::submit(&mut txn, &path.id).await?;

    txn.commit().await?;
    Ok(CheckActionResponse { success: true })
}

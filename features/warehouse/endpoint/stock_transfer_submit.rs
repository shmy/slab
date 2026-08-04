//! 提交调拨单。

use crate::repository::stock_transfer_repository::StockTransferRepository;
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
pub(crate) struct TransferActionPath {
    pub id: ID,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct TransferActionResponse {
    pub success: bool,
}

#[utoipa::path(post, path = "/api/v1/stock-transfers/{id}/submit",
    operation_id = "stock_transfer_submit", tag = "stock-transfer",
    params(TransferActionPath),
    responses((status = 200, body = JsonResponse<TransferActionResponse>)),
    security(("bearerAuth" = [])))]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ValidPath(path): ValidPath<TransferActionPath>,
) -> JsonResponseType<TransferActionResponse> {
    let response = execute(&pg_pool, path).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip_all)]
#[inline]
async fn execute(
    pg_pool: &PgPool,
    path: TransferActionPath,
) -> rootcause::Result<TransferActionResponse> {
    let mut conn = pg_pool.acquire().await?;
    let mut txn = conn.begin().await?;

    StockTransferRepository::submit(&mut txn, &path.id).await?;

    txn.commit().await?;
    Ok(TransferActionResponse { success: true })
}

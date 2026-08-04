use axum::extract::State;
use db::PgPool;
use serde::{Deserialize, Serialize};
use shared_contract::value_object::id::ID;
use sqlx::Acquire;
use utoipa::{IntoParams, ToSchema};
use validify::Validify;
use web::extract::valid_path::ValidPath;
use web::response::json_response::{JsonResponse, JsonResponseType};

use crate::repository::item_repository::ItemRepository;

#[derive(Debug, Deserialize, Validify, IntoParams)]
#[into_params(parameter_in = Path)]
pub(crate) struct DeleteItemPath {
    pub id: ID,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct DeleteItemResponse {
    pub deleted: bool,
}

#[utoipa::path(
    delete,
    path = "/api/v1/items/{id}",
    operation_id = "item_delete",
    tag = "item",
    params(DeleteItemPath),
    responses((status = 200, body = JsonResponse<DeleteItemResponse>)),
    security(("bearerAuth" = []))
)]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ValidPath(path): ValidPath<DeleteItemPath>,
) -> JsonResponseType<DeleteItemResponse> {
    let response = execute(&pg_pool, path).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip(pg_pool))]
#[inline]
async fn execute(pg_pool: &PgPool, path: DeleteItemPath) -> rootcause::Result<DeleteItemResponse> {
    let mut conn = pg_pool.acquire().await?;
    let mut txn = conn.begin().await?;
    let deleted = ItemRepository::delete(&mut txn, &path.id).await?;
    txn.commit().await?;
    Ok(DeleteItemResponse { deleted })
}

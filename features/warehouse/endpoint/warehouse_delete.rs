use axum::extract::State;
use db::PgPool;
use serde::{Deserialize, Serialize};
use shared_contract::value_object::id::ID;
use sqlx::Acquire;
use utoipa::{IntoParams, ToSchema};
use validify::Validify;
use web::extract::valid_path::ValidPath;
use web::response::json_response::{JsonResponse, JsonResponseType};

use crate::repository::warehouse_repository::WarehouseRepository;

#[derive(Debug, Deserialize, Validify, IntoParams)]
#[into_params(parameter_in = Path)]
pub(crate) struct DeleteWarehousePath {
    pub id: ID,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct DeleteWarehouseResponse {
    pub deleted: bool,
}

#[utoipa::path(
    delete,
    path = "/api/v1/warehouses/{id}",
    operation_id = "warehouse_delete",
    tag = "warehouse",
    params(DeleteWarehousePath),
    responses((status = 200, body = JsonResponse<DeleteWarehouseResponse>)),
    security(("bearerAuth" = []))
)]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ValidPath(path): ValidPath<DeleteWarehousePath>,
) -> JsonResponseType<DeleteWarehouseResponse> {
    let response = execute(&pg_pool, path).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip(pg_pool))]
#[inline]
async fn execute(
    pg_pool: &PgPool,
    path: DeleteWarehousePath,
) -> rootcause::Result<DeleteWarehouseResponse> {
    let mut conn = pg_pool.acquire().await?;
    let mut txn = conn.begin().await?;
    let deleted = WarehouseRepository::delete(&mut txn, &path.id).await?;
    txn.commit().await?;
    Ok(DeleteWarehouseResponse { deleted })
}

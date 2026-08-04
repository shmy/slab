use axum::extract::State;
use db::PgPool;
use serde::{Deserialize, Serialize};
use shared_contract::value_object::id::ID;
use sqlx::Acquire;
use utoipa::{IntoParams, ToSchema};
use validify::Validify;
use web::extract::valid_path::ValidPath;
use web::response::json_response::{JsonResponse, JsonResponseType};

use crate::repository::supplier_repository::SupplierRepository;

#[derive(Debug, Deserialize, Validify, IntoParams)]
#[into_params(parameter_in = Path)]
pub(crate) struct DeleteSupplierPath {
    pub id: ID,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct DeleteSupplierResponse {
    pub deleted: bool,
}

#[utoipa::path(
    delete, path = "/api/v1/suppliers/{id}", operation_id = "supplier_delete", tag = "supplier",
    params(DeleteSupplierPath),
    responses((status = 200, body = JsonResponse<DeleteSupplierResponse>)),
    security(("bearerAuth" = []))
)]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ValidPath(path): ValidPath<DeleteSupplierPath>,
) -> JsonResponseType<DeleteSupplierResponse> {
    let response = execute(&pg_pool, path).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip(pg_pool))]
#[inline]
async fn execute(
    pg_pool: &PgPool,
    path: DeleteSupplierPath,
) -> rootcause::Result<DeleteSupplierResponse> {
    let mut conn = pg_pool.acquire().await?;
    let mut txn = conn.begin().await?;
    let deleted = SupplierRepository::delete(txn.as_mut(), &path.id).await?;
    txn.commit().await?;
    Ok(DeleteSupplierResponse { deleted })
}

use axum::extract::State;
use db::PgPool;
use serde::{Deserialize, Serialize};
use shared_contract::value_object::id::ID;
use sqlx::Acquire;
use utoipa::{IntoParams, ToSchema};
use validify::Validify;
use web::extract::valid_path::ValidPath;
use web::response::json_response::{JsonResponse, JsonResponseType};

use crate::repository::customer_repository::CustomerRepository;

#[derive(Debug, Deserialize, Validify, IntoParams)]
#[into_params(parameter_in = Path)]
pub(crate) struct DeleteCustomerPath {
    pub id: ID,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct DeleteCustomerResponse {
    pub deleted: bool,
}

#[utoipa::path(
    delete, path = "/api/v1/customers/{id}", operation_id = "customer_delete", tag = "customer",
    params(DeleteCustomerPath),
    responses((status = 200, body = JsonResponse<DeleteCustomerResponse>)),
    security(("bearerAuth" = []))
)]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ValidPath(path): ValidPath<DeleteCustomerPath>,
) -> JsonResponseType<DeleteCustomerResponse> {
    let response = execute(&pg_pool, path).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip(pg_pool))]
#[inline]
async fn execute(
    pg_pool: &PgPool,
    path: DeleteCustomerPath,
) -> rootcause::Result<DeleteCustomerResponse> {
    let mut conn = pg_pool.acquire().await?;
    let mut txn = conn.begin().await?;
    let deleted = CustomerRepository::delete(txn.as_mut(), &path.id).await?;
    txn.commit().await?;
    Ok(DeleteCustomerResponse { deleted })
}

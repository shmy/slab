use axum::extract::State;
use db::PgPool;
use serde::{Deserialize, Serialize};
use shared_contract::value_object::id::ID;
use shared_contract::value_object::phone_number::PhoneNumber;
use sqlx::Acquire;
use std::fmt;
use utoipa::{IntoParams, ToSchema};
use validify::Validify;
use web::extract::{valid_json::ValidJson, valid_path::ValidPath};
use web::response::json_response::{JsonResponse, JsonResponseType};

use crate::repository::supplier_repository::SupplierRepository;

#[derive(Debug, Deserialize, Validify, IntoParams)]
#[into_params(parameter_in = Path)]
pub(crate) struct UpdateSupplierPath {
    pub id: ID,
}

#[derive(Deserialize, Validify, ToSchema)]
pub(crate) struct UpdateSupplierRequest {
    pub name: Option<String>,
    pub contact_person: Option<Option<String>>,
    pub phone: Option<Option<PhoneNumber>>,
    pub address: Option<Option<String>>,
    pub payment_terms: Option<Option<String>>,
    pub is_active: Option<bool>,
}

impl fmt::Debug for UpdateSupplierRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UpdateSupplierRequest")
            .field("name", &self.name)
            .field("contact_person", &"<Redacted>")
            .field("phone", &self.phone)
            .field("address", &"<Redacted>")
            .field("payment_terms", &self.payment_terms)
            .field("is_active", &self.is_active)
            .finish()
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct UpdateSupplierResponse {
    pub updated: bool,
}

#[utoipa::path(
    patch, path = "/api/v1/suppliers/{id}", operation_id = "supplier_update", tag = "supplier",
    params(UpdateSupplierPath), request_body = UpdateSupplierRequest,
    responses((status = 200, body = JsonResponse<UpdateSupplierResponse>)),
    security(("bearerAuth" = []))
)]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ValidPath(path): ValidPath<UpdateSupplierPath>,
    ValidJson(request): ValidJson<UpdateSupplierRequest>,
) -> JsonResponseType<UpdateSupplierResponse> {
    let response = execute(&pg_pool, path, request).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip_all)]
#[inline]
async fn execute(
    pg_pool: &PgPool,
    path: UpdateSupplierPath,
    request: UpdateSupplierRequest,
) -> rootcause::Result<UpdateSupplierResponse> {
    let mut conn = pg_pool.acquire().await?;
    let mut txn = conn.begin().await?;
    let updated = SupplierRepository::update(txn.as_mut(), &path.id, &request).await?;
    txn.commit().await?;
    Ok(UpdateSupplierResponse { updated })
}

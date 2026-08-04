use axum::extract::State;
use db::PgPool;
use serde::{Deserialize, Serialize};
use shared_contract::value_object::id::ID;
use shared_contract::value_object::phone_number::PhoneNumber;
use supplier_contract::port::SupplierPort;
use utoipa::{IntoParams, ToSchema};
use validify::Validify;
use web::extract::valid_path::ValidPath;
use web::response::json_response::{JsonResponse, JsonResponseType};

#[derive(Debug, Deserialize, Validify, IntoParams)]
#[into_params(parameter_in = Path)]
pub(crate) struct GetSupplierPath {
    pub id: ID,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct GetSupplierResponse {
    pub id: ID,
    pub code: String,
    pub name: String,
    pub contact_person: Option<String>,
    pub phone: Option<PhoneNumber>,
    pub address: Option<String>,
    pub payment_terms: Option<String>,
    pub is_active: bool,
}

#[utoipa::path(
    get, path = "/api/v1/suppliers/{id}", operation_id = "supplier_get", tag = "supplier",
    params(GetSupplierPath),
    responses((status = 200, body = JsonResponse<GetSupplierResponse>)),
    security(("bearerAuth" = []))
)]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ValidPath(path): ValidPath<GetSupplierPath>,
) -> JsonResponseType<GetSupplierResponse> {
    let response = execute(&pg_pool, path).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip(pg_pool))]
#[inline]
async fn execute(
    pg_pool: &PgPool,
    path: GetSupplierPath,
) -> rootcause::Result<GetSupplierResponse> {
    let mut conn = pg_pool.acquire().await?;
    let supplier = SupplierPort::by_id(&mut conn, &path.id)
        .await?
        .ok_or(supplier_contract::error::SupplierError::NotFound)?;
    Ok(GetSupplierResponse {
        id: supplier.id,
        code: supplier.code,
        name: supplier.name,
        contact_person: supplier.contact_person,
        phone: supplier.phone,
        address: supplier.address,
        payment_terms: supplier.payment_terms,
        is_active: supplier.is_active,
    })
}

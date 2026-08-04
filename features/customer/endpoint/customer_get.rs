use axum::extract::State;
use customer_contract::port::CustomerPort;
use db::PgPool;
use serde::{Deserialize, Serialize};
use shared_contract::value_object::id::ID;
use shared_contract::value_object::phone_number::PhoneNumber;
use utoipa::{IntoParams, ToSchema};
use validify::Validify;
use web::extract::valid_path::ValidPath;
use web::response::json_response::{JsonResponse, JsonResponseType};

#[derive(Debug, Deserialize, Validify, IntoParams)]
#[into_params(parameter_in = Path)]
pub(crate) struct GetCustomerPath {
    pub id: ID,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct GetCustomerResponse {
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
    get, path = "/api/v1/customers/{id}", operation_id = "customer_get", tag = "customer",
    params(GetCustomerPath),
    responses((status = 200, body = JsonResponse<GetCustomerResponse>)),
    security(("bearerAuth" = []))
)]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ValidPath(path): ValidPath<GetCustomerPath>,
) -> JsonResponseType<GetCustomerResponse> {
    let response = execute(&pg_pool, path).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip(pg_pool))]
#[inline]
async fn execute(
    pg_pool: &PgPool,
    path: GetCustomerPath,
) -> rootcause::Result<GetCustomerResponse> {
    let mut conn = pg_pool.acquire().await?;
    let customer = CustomerPort::by_id(&mut conn, &path.id)
        .await?
        .ok_or(customer_contract::error::CustomerError::NotFound)?;
    Ok(GetCustomerResponse {
        id: customer.id,
        code: customer.code,
        name: customer.name,
        contact_person: customer.contact_person,
        phone: customer.phone,
        address: customer.address,
        payment_terms: customer.payment_terms,
        is_active: customer.is_active,
    })
}

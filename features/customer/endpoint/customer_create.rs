// customer endpoints follow same pattern as item but simpler.
// For development speed, these are minimal working copies.

use axum::extract::State;
use code_gen::CodeGen;
use customer_contract::entity::Customer;
use db::PgPool;
use serde::{Deserialize, Serialize};
use shared_contract::value_object::id::ID;
use shared_contract::value_object::phone_number::PhoneNumber;
use sqlx::Acquire;
use std::fmt;
use utoipa::ToSchema;
use validify::Validify;
use web::extract::valid_json::ValidJson;
use web::response::json_response::{JsonResponse, JsonResponseType};

use crate::repository::customer_repository::CustomerRepository;

#[derive(Deserialize, Validify, ToSchema)]
pub(crate) struct CreateCustomerRequest {
    pub name: String,
    pub contact_person: Option<String>,
    #[validify]
    pub phone: Option<PhoneNumber>,
    pub address: Option<String>,
    pub payment_terms: Option<String>,
}

impl fmt::Debug for CreateCustomerRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // 观测路径脱敏：contact_person / address 不落日志，phone 由 PhoneNumber 的 Debug 打码
        f.debug_struct("CreateCustomerRequest")
            .field("name", &self.name)
            .field("contact_person", &"<Redacted>")
            .field("phone", &self.phone)
            .field("address", &"<Redacted>")
            .field("payment_terms", &self.payment_terms)
            .finish()
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct CreateCustomerResponse {
    pub id: ID,
    pub code: String,
}

#[utoipa::path(
    post, path = "/api/v1/customers", operation_id = "customer_create", tag = "customer",
    request_body = CreateCustomerRequest,
    responses((status = 200, body = JsonResponse<CreateCustomerResponse>)),
    security(("bearerAuth" = []))
)]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ValidJson(request): ValidJson<CreateCustomerRequest>,
) -> JsonResponseType<CreateCustomerResponse> {
    let response = execute(&pg_pool, request).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip_all)]
#[inline]
async fn execute(
    pg_pool: &PgPool,
    request: CreateCustomerRequest,
) -> rootcause::Result<CreateCustomerResponse> {
    let id = ID::new();
    let mut conn = pg_pool.acquire().await?;
    let mut txn = conn.begin().await?;
    let seq = CodeGen::next_seq(txn.as_mut(), "seq_customer").await?;
    let code = format!("C-{:06}", seq);
    let customer = Customer {
        id,
        code: code.clone(),
        name: request.name,
        contact_person: request.contact_person,
        phone: request.phone,
        address: request.address,
        payment_terms: request.payment_terms,
        is_active: true,
    };
    CustomerRepository::create(txn.as_mut(), &customer).await?;
    txn.commit().await?;
    Ok(CreateCustomerResponse { id, code })
}

#[cfg(test)]
mod tests {
    use super::*;
    use appctx::testing;
    use migration::run_migrations;

    #[sqlx::test]
    async fn test_create_success(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool).await;
        let request = CreateCustomerRequest {
            name: "Test Customer".into(),
            contact_person: Some("Contact".into()),
            phone: Some(PhoneNumber::try_new("13800138000").unwrap()),
            address: None,
            payment_terms: None,
        };
        let response = execute(&state.pg_pool, request).await.unwrap();
        assert!(i64::from(response.id) > 0);
        assert!(response.code.starts_with("C-"));
    }
}

use axum::extract::State;
use code_gen::CodeGen;
use db::PgPool;
use serde::{Deserialize, Serialize};
use shared_contract::value_object::id::ID;
use shared_contract::value_object::phone_number::PhoneNumber;
use sqlx::Acquire;
use std::fmt;
use supplier_contract::entity::Supplier;
use utoipa::ToSchema;
use validify::Validify;
use web::extract::valid_json::ValidJson;
use web::response::json_response::{JsonResponse, JsonResponseType};

use crate::repository::supplier_repository::SupplierRepository;

#[derive(Deserialize, Validify, ToSchema)]
pub(crate) struct CreateSupplierRequest {
    pub name: String,
    pub contact_person: Option<String>,
    #[validify]
    pub phone: Option<PhoneNumber>,
    pub address: Option<String>,
    pub payment_terms: Option<String>,
}

impl fmt::Debug for CreateSupplierRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CreateSupplierRequest")
            .field("name", &self.name)
            .field("contact_person", &"<Redacted>")
            .field("phone", &self.phone)
            .field("address", &"<Redacted>")
            .field("payment_terms", &self.payment_terms)
            .finish()
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct CreateSupplierResponse {
    pub id: ID,
    pub code: String,
}

#[utoipa::path(
    post, path = "/api/v1/suppliers", operation_id = "supplier_create", tag = "supplier",
    request_body = CreateSupplierRequest,
    responses((status = 200, body = JsonResponse<CreateSupplierResponse>)),
    security(("bearerAuth" = []))
)]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ValidJson(request): ValidJson<CreateSupplierRequest>,
) -> JsonResponseType<CreateSupplierResponse> {
    let response = execute(&pg_pool, request).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip_all)]
#[inline]
async fn execute(
    pg_pool: &PgPool,
    request: CreateSupplierRequest,
) -> rootcause::Result<CreateSupplierResponse> {
    let id = ID::new();
    let mut conn = pg_pool.acquire().await?;
    let mut txn = conn.begin().await?;
    let seq = CodeGen::next_seq(txn.as_mut(), "seq_supplier").await?;
    let code = format!("S-{:06}", seq);
    let supplier = Supplier {
        id,
        code: code.clone(),
        name: request.name,
        contact_person: request.contact_person,
        phone: request.phone,
        address: request.address,
        payment_terms: request.payment_terms,
        is_active: true,
    };
    SupplierRepository::create(txn.as_mut(), &supplier).await?;
    txn.commit().await?;
    Ok(CreateSupplierResponse { id, code })
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
        let request = CreateSupplierRequest {
            name: "Test Supplier".into(),
            contact_person: Some("Contact".into()),
            phone: Some(PhoneNumber::try_new("13900139000").unwrap()),
            address: None,
            payment_terms: None,
        };
        let response = execute(&state.pg_pool, request).await.unwrap();
        assert!(i64::from(response.id) > 0);
        assert!(response.code.starts_with("S-"));
    }
}

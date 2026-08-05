use audit_contract::AuditService;
use axum::extract::State;
use customer_contract::error::CustomerError;
use customer_contract::port::CustomerPort;
use db::PgPool;
use http_auth::extract::operator::OperatorContext;
use serde::{Deserialize, Serialize};
use shared_contract::value_object::id::ID;
use shared_contract::value_object::phone_number::PhoneNumber;
use sqlx::Acquire;
use std::fmt;
use utoipa::{IntoParams, ToSchema};
use validify::Validify;
use web::extract::{valid_json::ValidJson, valid_path::ValidPath};
use web::response::json_response::{JsonResponse, JsonResponseType};

use crate::repository::customer_repository::CustomerRepository;

#[derive(Debug, Deserialize, Validify, IntoParams)]
#[into_params(parameter_in = Path)]
pub(crate) struct UpdateCustomerPath {
    pub id: ID,
}

#[derive(Deserialize, Validify, ToSchema)]
pub(crate) struct UpdateCustomerRequest {
    pub name: Option<String>,
    pub contact_person: Option<Option<String>>,
    pub phone: Option<Option<PhoneNumber>>,
    pub address: Option<Option<String>>,
    pub payment_terms: Option<Option<String>>,
    pub is_active: Option<bool>,
}

impl fmt::Debug for UpdateCustomerRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UpdateCustomerRequest")
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
pub(crate) struct UpdateCustomerResponse {
    pub updated: bool,
}

#[utoipa::path(
    patch, path = "/api/v1/customers/{id}", operation_id = "customer_update", tag = "customer",
    params(UpdateCustomerPath), request_body = UpdateCustomerRequest,
    responses((status = 200, body = JsonResponse<UpdateCustomerResponse>)),
    security(("bearerAuth" = []))
)]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ctx: OperatorContext,
    ValidPath(path): ValidPath<UpdateCustomerPath>,
    ValidJson(request): ValidJson<UpdateCustomerRequest>,
) -> JsonResponseType<UpdateCustomerResponse> {
    let response = execute(&pg_pool, ctx, path, request).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip_all)]
#[inline]
async fn execute(
    pg_pool: &PgPool,
    ctx: OperatorContext,
    path: UpdateCustomerPath,
    request: UpdateCustomerRequest,
) -> rootcause::Result<UpdateCustomerResponse> {
    let mut conn = pg_pool.acquire().await?;
    let mut txn = conn.begin().await?;
    let before = CustomerPort::by_id(&mut txn, &path.id)
        .await?
        .ok_or(CustomerError::NotFound)?;
    let updated = CustomerRepository::update(txn.as_mut(), &path.id, &request).await?;
    let after = CustomerPort::by_id(&mut txn, &path.id)
        .await?
        .ok_or(CustomerError::NotFound)?;
    AuditService::record_updated(&mut txn, "customer", &path.id, &ctx, &before, &after).await?;
    txn.commit().await?;
    Ok(UpdateCustomerResponse { updated })
}

#[cfg(test)]
mod tests {
    use super::*;
    use appctx::testing;
    use migration::run_migrations;

    #[sqlx::test]
    async fn test_update_success(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool).await;
        let customer_id =
            crate::tests::insert_test_customer(&state.pg_pool, "C-TEST-01", "Old Name").await;
        let response = execute(
            &state.pg_pool,
            crate::tests::test_operator_context(),
            UpdateCustomerPath { id: customer_id },
            UpdateCustomerRequest {
                name: Some("New Name".into()),
                contact_person: None,
                phone: None,
                address: None,
                payment_terms: None,
                is_active: None,
            },
        )
        .await
        .unwrap();
        assert!(response.updated);

        // 变更历史：update 类型，before/after 快照
        let mut conn = state.pg_pool.acquire().await.unwrap();
        let audit_row = sqlx::query!(
            r#"SELECT action, before, after FROM audit_logs WHERE entity_id = $1"#,
            *customer_id
        )
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        assert_eq!(audit_row.action, 2); // Updated
        let before: serde_json::Value = audit_row.before.unwrap();
        let after: serde_json::Value = audit_row.after.unwrap();
        assert_eq!(before["name"], "Old Name");
        assert_eq!(after["name"], "New Name");
        assert_eq!(before["is_active"], true);
        assert_eq!(after["is_active"], true);
    }
}

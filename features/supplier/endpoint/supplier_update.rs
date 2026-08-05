use audit_contract::AuditService;
use axum::extract::State;
use db::PgPool;
use http_auth::extract::operator::OperatorContext;
use serde::{Deserialize, Serialize};
use shared_contract::value_object::id::ID;
use shared_contract::value_object::phone_number::PhoneNumber;
use sqlx::Acquire;
use std::fmt;
use supplier_contract::port::SupplierPort;
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
    ctx: OperatorContext,
    ValidPath(path): ValidPath<UpdateSupplierPath>,
    ValidJson(request): ValidJson<UpdateSupplierRequest>,
) -> JsonResponseType<UpdateSupplierResponse> {
    let response = execute(&pg_pool, ctx, path, request).await?;
    JsonResponse::ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests;
    use appctx::testing;
    use migration::run_migrations;

    #[sqlx::test]
    async fn test_update_success(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool).await;
        let supplier_id = tests::insert_test_supplier(&state.pg_pool).await;
        let response = execute(
            &state.pg_pool,
            tests::test_operator_context(),
            UpdateSupplierPath { id: supplier_id },
            UpdateSupplierRequest {
                name: Some("Updated Supplier".into()),
                contact_person: None,
                phone: None,
                address: None,
                payment_terms: None,
                is_active: Some(false),
            },
        )
        .await
        .unwrap();
        assert!(response.updated);

        // 变更历史：update 类型，before/after 快照
        let mut conn = state.pg_pool.acquire().await.unwrap();
        let audit_row = sqlx::query!(
            r#"SELECT action, before, after FROM audit_logs WHERE entity_id = $1"#,
            *supplier_id
        )
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        assert_eq!(audit_row.action, 2); // Updated
        let before: serde_json::Value = audit_row.before.unwrap();
        let after: serde_json::Value = audit_row.after.unwrap();
        assert_eq!(before["name"], "Test Supplier");
        assert_eq!(after["name"], "Updated Supplier");
        assert_eq!(before["is_active"], true);
        assert_eq!(after["is_active"], false);
    }

    #[sqlx::test]
    async fn test_update_not_found(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool).await;
        let err = execute(
            &state.pg_pool,
            tests::test_operator_context(),
            UpdateSupplierPath {
                id: ID::from(999_i64),
            },
            UpdateSupplierRequest {
                name: Some("N".into()),
                contact_person: None,
                phone: None,
                address: None,
                payment_terms: None,
                is_active: None,
            },
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("supplier_not_found"));

        // 不存在则失败，不产生变更记录
        let mut conn = state.pg_pool.acquire().await.unwrap();
        let count = sqlx::query!(
            r#"SELECT COUNT(*) AS "count!" FROM audit_logs WHERE entity_id = $1"#,
            999i64
        )
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        assert_eq!(count.count, 0);
    }
}

#[tracing::instrument(skip_all)]
#[inline]
async fn execute(
    pg_pool: &PgPool,
    ctx: OperatorContext,
    path: UpdateSupplierPath,
    request: UpdateSupplierRequest,
) -> rootcause::Result<UpdateSupplierResponse> {
    let mut conn = pg_pool.acquire().await?;
    let mut txn = conn.begin().await?;
    // 变更历史：更新前读旧值作为 before；不存在则直接失败，不产生记录
    let before = SupplierPort::by_id(&mut txn, &path.id)
        .await?
        .ok_or(supplier_contract::error::SupplierError::NotFound)?;
    let updated = SupplierRepository::update(txn.as_mut(), &path.id, &request).await?;
    // 变更历史：写后在同一事务内重读作为 after
    let after = SupplierPort::by_id(&mut txn, &path.id)
        .await?
        .ok_or(supplier_contract::error::SupplierError::NotFound)?;
    AuditService::record_updated(&mut txn, "supplier", &path.id, &ctx, &before, &after).await?;
    txn.commit().await?;
    Ok(UpdateSupplierResponse { updated })
}

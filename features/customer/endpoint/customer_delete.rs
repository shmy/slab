use audit_contract::AuditService;
use axum::extract::State;
use customer_contract::port::CustomerPort;
use db::PgPool;
use http_auth::extract::operator::OperatorContext;
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
    ctx: OperatorContext,
    ValidPath(path): ValidPath<DeleteCustomerPath>,
) -> JsonResponseType<DeleteCustomerResponse> {
    let response = execute(&pg_pool, ctx, path).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip(pg_pool))]
#[inline]
async fn execute(
    pg_pool: &PgPool,
    ctx: OperatorContext,
    path: DeleteCustomerPath,
) -> rootcause::Result<DeleteCustomerResponse> {
    let mut conn = pg_pool.acquire().await?;
    let mut txn = conn.begin().await?;

    // 删除前读旧值用于变更历史；客户不存在时幂等删除，不产生记录
    let before = CustomerPort::by_id(&mut txn, &path.id).await?;
    let deleted = CustomerRepository::delete(txn.as_mut(), &path.id).await?;
    if let Some(before) = before {
        AuditService::record_deleted(&mut txn, "customer", &path.id, &ctx, &before).await?;
    }
    txn.commit().await?;
    Ok(DeleteCustomerResponse { deleted })
}

#[cfg(test)]
mod tests {
    use super::*;
    use appctx::testing;
    use migration::run_migrations;

    #[sqlx::test]
    async fn test_delete_success(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool).await;
        let customer_id =
            crate::tests::insert_test_customer(&state.pg_pool, "C-TEST-02", "To Delete").await;
        let response = execute(
            &state.pg_pool,
            crate::tests::test_operator_context(),
            DeleteCustomerPath { id: customer_id },
        )
        .await
        .unwrap();
        assert!(response.deleted);

        // 变更历史：delete 类型，after 为空
        let mut conn = state.pg_pool.acquire().await.unwrap();
        let audit_row = sqlx::query!(
            r#"SELECT action, before, after FROM audit_logs WHERE entity_id = $1"#,
            *customer_id
        )
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        assert_eq!(audit_row.action, 3); // Deleted
        let before: serde_json::Value = audit_row.before.unwrap();
        assert_eq!(before["name"], "To Delete");
        assert!(audit_row.after.is_none());
    }

    #[sqlx::test]
    async fn test_delete_nonexistent_no_audit(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool).await;
        let response = execute(
            &state.pg_pool,
            crate::tests::test_operator_context(),
            DeleteCustomerPath {
                id: ID::from(999_i64),
            },
        )
        .await
        .unwrap();
        assert!(!response.deleted);

        // 不存在的客户：不产生变更记录
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

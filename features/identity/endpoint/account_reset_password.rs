use crate::repository::account_repository::AccountRepository;
use appctx::PgPool;
use audit_contract::AuditService;
use axum::extract::State;
use http_auth::extract::operator::OperatorContext;
use identity_contract::error::IdentityError;
use identity_contract::port::AccountPort;
use identity_contract::value_object::hashed_password::HashedPassword;
use identity_contract::value_object::password::Password;
use serde::{Deserialize, Serialize};
use shared_contract::value_object::id::ID;
use utoipa::{IntoParams, ToSchema};
use validify::Validify;
use web::extract::valid_json::ValidJson;
use web::extract::valid_path::ValidPath;
use web::response::json_response::{JsonResponse, JsonResponseType};

#[derive(Debug, Deserialize, Validify, IntoParams)]
#[into_params(parameter_in = Path)]
pub struct ResetPasswordPath {
    pub id: ID,
}

#[derive(Debug, Deserialize, Validify, ToSchema)]
pub struct ResetPasswordRequest {
    #[validify]
    pub new_password: Password,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ResetPasswordResponse {
    #[schema(value_type = bool, example = true)]
    pub updated: bool,
}

#[utoipa::path(
    patch,
    path = "/api/v1/accounts/password/{id}",
    operation_id = "account_reset_password",
    tag = "account",
    params(ResetPasswordPath),
    request_body = ResetPasswordRequest,
    responses((status = 200, body = JsonResponse<ResetPasswordResponse>)),
    security(("bearerAuth" = []))
)]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ctx: OperatorContext,
    ValidPath(path): ValidPath<ResetPasswordPath>,
    ValidJson(request): ValidJson<ResetPasswordRequest>,
) -> JsonResponseType<ResetPasswordResponse> {
    let response = execute(&pg_pool, ctx, path, request).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip(pg_pool))]
#[inline]
async fn execute(
    pg_pool: &PgPool,
    ctx: OperatorContext,
    path: ResetPasswordPath,
    request: ResetPasswordRequest,
) -> rootcause::Result<ResetPasswordResponse> {
    let new_hashed = HashedPassword::try_new(&request.new_password)?;
    let mut conn = pg_pool.acquire().await?;
    // 端点无显式事务：before/after 读取与审计写入共用同一连接（各自隐式事务）
    let before = AccountPort::by_id(&mut conn, &path.id).await?;
    // 特权账号受保护：不可被重置密码（前端同时禁用 UI，后端强校验双保险）
    if before.privileged {
        return Err(IdentityError::AccountProtected.into());
    }
    AccountRepository::update_password(&mut conn, &path.id, &new_hashed).await?;
    let after = AccountPort::by_id(&mut conn, &path.id).await?;
    AuditService::record_updated(&mut conn, "account", &path.id, &ctx, &before, &after).await?;
    Ok(ResetPasswordResponse { updated: true })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests;
    use appctx::testing;
    use migration::run_migrations;

    #[sqlx::test]
    async fn test_reset_password_success(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool).await;
        let account_id = tests::insert_test_account(&state.pg_pool, "13900001711").await;
        let response = execute(
            &state.pg_pool,
            tests::test_operator_context(),
            ResetPasswordPath { id: account_id },
            ResetPasswordRequest {
                new_password: Password::new_unchecked("reset1234".to_string()),
            },
        )
        .await
        .unwrap();
        assert!(response.updated);

        let mut conn = state.pg_pool.acquire().await.unwrap();
        let row = sqlx::query!(
            r#"SELECT password FROM accounts WHERE id = $1"#,
            &*account_id
        )
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        let hashed = HashedPassword::new_unchecked(row.password);
        assert!(hashed.verify("reset1234").is_ok());
        assert!(hashed.verify("test1234").is_err());

        // 变更历史：update 类型，快照不含敏感字段
        let audit_row = sqlx::query!(
            r#"SELECT action, before, after FROM audit_logs WHERE entity_id = $1"#,
            *account_id
        )
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        assert_eq!(audit_row.action, 2); // Updated
        let before: serde_json::Value = audit_row.before.unwrap();
        let after: serde_json::Value = audit_row.after.unwrap();
        assert_eq!(before["name"], "test-13900001711");
        assert_eq!(before["name"], after["name"]);
        assert!(before.get("password").is_none());
        assert!(after.get("password").is_none());
        assert!(before.get("version").is_none());
        assert!(after.get("version").is_none());
    }

    #[sqlx::test]
    async fn test_reset_password_not_found(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool).await;
        let err = execute(
            &state.pg_pool,
            tests::test_operator_context(),
            ResetPasswordPath {
                id: ID::from(999_i64),
            },
            ResetPasswordRequest {
                new_password: Password::new_unchecked("reset1234".to_string()),
            },
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("account_not_found"));

        // 账户不存在：更新未发生，不产生变更记录
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

use crate::repository::account_repository::AccountRepository;
use appctx::PgPool;
use audit_contract::AuditService;
use axum::extract::State;
use http_auth::extract::operator::OperatorContext;
use identity_contract::port::AccountPort;
use identity_contract::value_object::hashed_password::HashedPassword;
use identity_contract::value_object::password::Password;
use serde::{Deserialize, Serialize};
use sqlx::Acquire;
use utoipa::ToSchema;
use validify::Validify;
use web::extract::valid_json::ValidJson;
use web::response::json_response::{JsonResponse, JsonResponseType};

#[derive(Debug, Deserialize, Validify, ToSchema)]
pub struct UpdatePasswordRequest {
    #[validify]
    pub old_password: Password,
    #[validify]
    pub new_password: Password,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct UpdatePasswordResponse {
    #[schema(value_type = bool, example = true)]
    pub updated: bool,
}

#[utoipa::path(
    patch,
    path = "/api/v1/identity/password",
    operation_id = "identity_update_password",
    tag = "identity",
    request_body = UpdatePasswordRequest,
    responses((status = 200, body = JsonResponse<UpdatePasswordResponse>)),
    security(("bearerAuth" = []))
)]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ctx: OperatorContext,
    ValidJson(request): ValidJson<UpdatePasswordRequest>,
) -> JsonResponseType<UpdatePasswordResponse> {
    let response = execute(&pg_pool, ctx, request).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip(pg_pool))]
#[inline]
async fn execute(
    pg_pool: &PgPool,
    ctx: OperatorContext,
    request: UpdatePasswordRequest,
) -> rootcause::Result<UpdatePasswordResponse> {
    // 本人改密：被修改的账户即当前操作人
    let account_id = ctx.operator_id;
    let mut conn = pg_pool.acquire().await?;
    let mut txn = conn.begin().await?;
    let before = AccountPort::by_id(&mut txn, &account_id).await?;
    let old_hashed = AccountRepository::get_password_hash(&mut txn, &account_id).await?;
    old_hashed.verify(&request.old_password)?;
    let new_hashed = HashedPassword::try_new(&request.new_password)?;
    AccountRepository::update_password(&mut txn, &account_id, &new_hashed).await?;
    let after = AccountPort::by_id(&mut txn, &account_id).await?;
    AuditService::record_updated(&mut txn, "account", &account_id, &ctx, &before, &after).await?;
    txn.commit().await?;
    Ok(UpdatePasswordResponse { updated: true })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests;
    use appctx::testing;
    use migration::run_migrations;
    use shared_contract::value_object::operator::Operator;

    #[sqlx::test]
    async fn test_update_password_success(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool).await;
        let account_id = tests::insert_test_account(&state.pg_pool, "13900001701").await;
        // 本人改密：操作人即被改账户
        let ctx = OperatorContext(Operator {
            operator_id: account_id,
            ip: None,
            user_agent: None,
        });
        let response = execute(
            &state.pg_pool,
            ctx,
            UpdatePasswordRequest {
                old_password: Password::new_unchecked("test1234".to_string()),
                new_password: Password::new_unchecked("newpass1234".to_string()),
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
        assert!(hashed.verify("newpass1234").is_ok());
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
        assert_eq!(before["name"], "test-13900001701");
        assert_eq!(before["name"], after["name"]);
        assert!(before.get("password").is_none());
        assert!(after.get("password").is_none());
        assert!(before.get("version").is_none());
        assert!(after.get("version").is_none());
    }

    #[sqlx::test]
    async fn test_update_password_incorrect_old_password(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool).await;
        let account_id = tests::insert_test_account(&state.pg_pool, "13900001702").await;
        // 本人改密：操作人即被改账户
        let ctx = OperatorContext(Operator {
            operator_id: account_id,
            ip: None,
            user_agent: None,
        });
        let err = execute(
            &state.pg_pool,
            ctx,
            UpdatePasswordRequest {
                old_password: Password::new_unchecked("wrong1234".to_string()),
                new_password: Password::new_unchecked("newpass1234".to_string()),
            },
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("account_password_incorrect"));

        let mut conn = state.pg_pool.acquire().await.unwrap();
        let row = sqlx::query!(
            r#"SELECT password FROM accounts WHERE id = $1"#,
            &*account_id
        )
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        let hashed = HashedPassword::new_unchecked(row.password);
        assert!(hashed.verify("test1234").is_ok());

        // 旧密码校验失败：更新未发生，不产生变更记录
        let count = sqlx::query!(
            r#"SELECT COUNT(*) AS "count!" FROM audit_logs WHERE entity_id = $1"#,
            *account_id
        )
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        assert_eq!(count.count, 0);
    }
}

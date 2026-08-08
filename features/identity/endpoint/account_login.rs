use crate::shared::token_ops;
use std::time::Duration;

use appctx::EventBus;
use appctx::{KvBackend, PgPool, TokenBundle, TokenHelper};
use axum::extract::State;
use identity_contract::error::IdentityError;
use identity_contract::events::AccountLoggedInEvent;
use identity_contract::value_object::hashed_password::HashedPassword;
use identity_contract::value_object::password::Password;
use serde::{Deserialize, Serialize};
use shared_contract::value_object::id::ID;
use shared_contract::value_object::phone_number::PhoneNumber;
use utoipa::ToSchema;
use validify::Validify;
use web::extract::valid_json::ValidJson;
use web::response::json_response::{JsonResponse, JsonResponseType};

#[derive(Debug, Deserialize, Validify, ToSchema)]
pub struct LoginRequest {
    #[validify]
    pub phone: PhoneNumber,
    #[schema(example = "admin123!")]
    #[validify]
    pub password: Password,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct LoginResponse {
    #[schema(value_type = String, example = "1234567890123456789")]
    pub(crate) access_token: String,
    #[schema(value_type = String, example = "1234567890123456789")]
    pub(crate) refresh_token: String,
    #[schema(value_type = String, example = "Bearer")]
    pub(crate) token_type: String,
    #[schema(value_type = u64, example = 100)]
    pub(crate) expires_in: u64,
}

impl LoginResponse {
    pub fn bearer(access_token: String, refresh_token: String, expires_in: u64) -> Self {
        Self {
            access_token,
            refresh_token,
            token_type: "Bearer".to_string(),
            expires_in,
        }
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/identity/login",
    operation_id = "identity_login",
    tag = "identity",
    request_body = LoginRequest,
    responses((status = 200, body = JsonResponse<LoginResponse>))
)]
#[tracing::instrument(skip(pg_pool, kv, bus))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    State(kv): State<KvBackend>,
    State(bus): State<EventBus>,
    State(token_bundle): State<TokenBundle>,
    ValidJson(request): ValidJson<LoginRequest>,
) -> JsonResponseType<LoginResponse> {
    let response = execute(&pg_pool, &kv, &bus, token_bundle.account(), request).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip(pg_pool, kv, bus))]
#[inline]
async fn execute(
    pg_pool: &PgPool,
    kv: &KvBackend,
    bus: &EventBus,
    token_helper: &TokenHelper,
    request: LoginRequest,
) -> rootcause::Result<LoginResponse> {
    let mut conn = pg_pool.acquire().await?;
    let row = sqlx::query_as!(
        LoginRow,
        r#"SELECT id as "id: ID", password FROM accounts WHERE phone = $1"#,
        &*request.phone
    )
    .fetch_optional(&mut *conn)
    .await?
    .ok_or(IdentityError::AccountInvalidCredentials)?;

    let hashed = HashedPassword::new_unchecked(row.password);
    if hashed.verify(&request.password).is_err() {
        return Err(IdentityError::AccountInvalidCredentials.into());
    }

    let id = row.id;
    let tokens = token_ops::issue_tokens(kv, token_helper, &id).await?;
    bus.publish_with_delay(&AccountLoggedInEvent { id }, Duration::from_secs(10))
        .await?;
    Ok(LoginResponse::bearer(
        tokens.access_token,
        tokens.refresh_token,
        tokens.expires_in,
    ))
}

struct LoginRow {
    id: ID,
    password: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests;
    use appctx::testing;
    use identity_contract::{events::AccountLoggedInEvent, value_object::password::Password};
    use migration::run_migrations;
    use shared_contract::event::Event as _;

    #[sqlx::test]
    async fn test_login_success(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool).await;
        let account_id = tests::insert_test_account(&state.pg_pool, "13900001102").await;
        let response = execute(
            &state.pg_pool,
            &state.kv,
            &state.bus,
            state.token_bundle.account(),
            LoginRequest {
                phone: PhoneNumber::try_new("13900001102").unwrap(),
                password: Password::new_unchecked("test1234".to_string()),
            },
        )
        .await
        .unwrap();
        assert!(response.access_token.len() > 20);
        assert!(response.refresh_token.len() > 10);
        assert_eq!(response.token_type, "Bearer");
        assert!(response.expires_in > 0);

        let mut conn = state.pg_pool.acquire().await.unwrap();
        let row = sqlx::query!(
            r#"
                SELECT topic, payload
                FROM _pg_queues
                WHERE topic = $1 AND status = 1
                ORDER BY id DESC
                LIMIT 1
                "#,
            AccountLoggedInEvent::TOPIC
        )
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        assert_eq!(row.topic, AccountLoggedInEvent::TOPIC);
        let payload: serde_json::Value = serde_json::from_str(&row.payload).unwrap();
        assert_eq!(
            payload["id"],
            serde_json::Value::String(account_id.to_string())
        );
    }

    #[sqlx::test]
    async fn test_login_wrong_password(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool).await;
        tests::insert_test_account(&state.pg_pool, "13900001103").await;
        let err = execute(
            &state.pg_pool,
            &state.kv,
            &state.bus,
            state.token_bundle.account(),
            LoginRequest {
                phone: PhoneNumber::try_new("13900001103").unwrap(),
                password: Password::new_unchecked("wrong_pass".to_string()),
            },
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("account_invalid_credentials"));
    }

    #[sqlx::test]
    async fn test_login_not_found(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool).await;
        let err = execute(
            &state.pg_pool,
            &state.kv,
            &state.bus,
            state.token_bundle.account(),
            LoginRequest {
                phone: PhoneNumber::try_new("13900001104").unwrap(),
                password: Password::new_unchecked("test1234".to_string()),
            },
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("account_invalid_credentials"));
    }
}

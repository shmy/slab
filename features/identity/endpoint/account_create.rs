use crate::repository::account_repository::AccountRepository;
use axum::extract::State;
use db::PgPool;
use identity_contract::entity::account::Account;
use identity_contract::events::AccountCreatedEvent;
use identity_contract::value_object::hashed_password::HashedPassword;
use identity_contract::value_object::password::Password;
use queue::enqueue_event;
use serde::{Deserialize, Serialize};
use shared_contract::value_object::id::ID;
use shared_contract::value_object::phone_number::PhoneNumber;
use sqlx::Acquire;
use utoipa::ToSchema;
use validify::Validify;
use web::extract::valid_json::ValidJson;
use web::response::json_response::{JsonResponse, JsonResponseType};

#[derive(Debug, Deserialize, Validify, ToSchema)]
pub(crate) struct CreateAccountRequest {
    #[schema(example = "Tom")]
    pub name: String,
    #[validify]
    pub phone: PhoneNumber,
    #[schema(example = "admin123!")]
    #[validify]
    pub password: Password,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct CreateAccountResponse {
    pub id: ID,
    pub name: String,
}

#[utoipa::path(
    post,
    path = "/api/v1/accounts",
    operation_id = "account_create",
    tag = "account",
    request_body = CreateAccountRequest,
    responses((status = 200, body = JsonResponse<CreateAccountResponse>)),
    security(("bearerAuth" = []))
)]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ValidJson(request): ValidJson<CreateAccountRequest>,
) -> JsonResponseType<CreateAccountResponse> {
    let response = execute(&pg_pool, request).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip(pg_pool))]
#[inline]
async fn execute(
    pg_pool: &PgPool,
    request: CreateAccountRequest,
) -> rootcause::Result<CreateAccountResponse> {
    let id = ID::new();
    let name = request.name.trim().to_string();
    let password = HashedPassword::try_new(&request.password)?;
    let account = Account {
        id,
        name,
        phone: request.phone,
        password,
        privileged: false,
        version: 1,
    };
    let mut conn = pg_pool.acquire().await?;
    let mut txn = conn.begin().await?;
    AccountRepository::create(&mut txn, &account).await?;
    enqueue_event(txn.as_mut(), &AccountCreatedEvent { id }).await?;
    txn.commit().await?;
    Ok(CreateAccountResponse {
        id,
        name: account.name,
    })
}

#[cfg(test)]
mod tests {
    use identity_contract::{events::AccountCreatedEvent, value_object::password::Password};
    use shared_contract::event::Event as _;

    use super::*;

    use appctx::testing;
    use migration::run_migrations;

    #[sqlx::test]
    async fn test_create_success(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool).await;
        let request = CreateAccountRequest {
            name: "Tom".to_string(),
            phone: PhoneNumber::try_new("13900001201").unwrap(),
            password: Password::new_unchecked("admin123!".to_string()),
        };
        let response = execute(&state.pg_pool, request).await.unwrap();
        assert!(i64::from(response.id) > 0);

        let mut conn = state.pg_pool.acquire().await.unwrap();
        let row = sqlx::query!(
            r#"SELECT topic, payload FROM queues WHERE status = 1 ORDER BY id DESC LIMIT 1"#
        )
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        let topic = row.topic;
        let payload_str = row.payload;
        let payload: serde_json::Value = serde_json::from_str(&payload_str).unwrap();
        assert_eq!(topic, AccountCreatedEvent::TOPIC);
        assert_eq!(
            payload["id"],
            serde_json::Value::String(response.id.to_string())
        );
    }
}

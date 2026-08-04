use crate::repository::account_repository::AccountRepository;
use appctx::PgPool;
use axum::extract::State;
use http_auth::extract::authed_account::AuthedAccount;
use identity_contract::value_object::hashed_password::HashedPassword;
use identity_contract::value_object::password::Password;
use serde::{Deserialize, Serialize};
use shared_contract::value_object::id::ID;
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
    AuthedAccount(account_id): AuthedAccount,
    State(pg_pool): State<PgPool>,
    ValidJson(request): ValidJson<UpdatePasswordRequest>,
) -> JsonResponseType<UpdatePasswordResponse> {
    let response = execute(&pg_pool, account_id, request).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip(pg_pool))]
#[inline]
async fn execute(
    pg_pool: &PgPool,
    account_id: ID,
    request: UpdatePasswordRequest,
) -> rootcause::Result<UpdatePasswordResponse> {
    let mut conn = pg_pool.acquire().await?;
    let mut txn = conn.begin().await?;
    let old_hashed = AccountRepository::get_password_hash(&mut txn, &account_id).await?;
    old_hashed.verify(&request.old_password)?;
    let new_hashed = HashedPassword::try_new(&request.new_password)?;
    AccountRepository::update_password(&mut txn, &account_id, &new_hashed).await?;
    txn.commit().await?;
    Ok(UpdatePasswordResponse { updated: true })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests;
    use appctx::testing;
    use migration::run_migrations;

    #[sqlx::test]
    async fn test_update_password_success(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool).await;
        let account_id = tests::insert_test_account(&state.pg_pool, "13900001701").await;
        let response = execute(
            &state.pg_pool,
            account_id,
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
    }

    #[sqlx::test]
    async fn test_update_password_incorrect_old_password(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool).await;
        let account_id = tests::insert_test_account(&state.pg_pool, "13900001702").await;
        let err = execute(
            &state.pg_pool,
            account_id,
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
    }
}

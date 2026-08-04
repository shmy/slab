use crate::repository::account_repository::AccountRepository;
use appctx::PgPool;
use axum::extract::State;
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
    ValidPath(path): ValidPath<ResetPasswordPath>,
    ValidJson(request): ValidJson<ResetPasswordRequest>,
) -> JsonResponseType<ResetPasswordResponse> {
    let response = execute(&pg_pool, path, request).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip(pg_pool))]
#[inline]
async fn execute(
    pg_pool: &PgPool,
    path: ResetPasswordPath,
    request: ResetPasswordRequest,
) -> rootcause::Result<ResetPasswordResponse> {
    let new_hashed = HashedPassword::try_new(&request.new_password)?;
    let mut conn = pg_pool.acquire().await?;
    AccountRepository::update_password(&mut conn, &path.id, &new_hashed).await?;
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
    }

    #[sqlx::test]
    async fn test_reset_password_not_found(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool).await;
        let err = execute(
            &state.pg_pool,
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
    }
}

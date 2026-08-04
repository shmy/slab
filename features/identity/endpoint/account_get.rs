use axum::extract::State;
use db::PgPool;
use identity_contract::port::AccountPort;
use serde::{Deserialize, Serialize};
use shared_contract::value_object::id::ID;
use shared_contract::value_object::phone_number::PhoneNumber;
use utoipa::{IntoParams, ToSchema};
use validify::Validify;
use web::extract::valid_path::ValidPath;
use web::response::json_response::{JsonResponse, JsonResponseType};

#[derive(Debug, Deserialize, Validify, IntoParams)]
#[into_params(parameter_in = Path)]
pub(crate) struct GetAccountPath {
    pub id: ID,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct GetAccountResponse {
    pub id: ID,
    #[schema(example = "Tom")]
    pub name: String,
    pub phone: PhoneNumber,
    pub privileged: bool,
}

#[utoipa::path(
    get,
    path = "/api/v1/accounts/{id}",
    operation_id = "account_get",
    tag = "account",
    params(GetAccountPath),
    responses((status = 200, body = JsonResponse<GetAccountResponse>)),
    security(("bearerAuth" = []))
)]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ValidPath(path): ValidPath<GetAccountPath>,
) -> JsonResponseType<GetAccountResponse> {
    let response = execute(&pg_pool, path).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip(pg_pool))]
#[inline]
async fn execute(pg_pool: &PgPool, path: GetAccountPath) -> rootcause::Result<GetAccountResponse> {
    let mut conn = pg_pool.acquire().await?;
    let account = AccountPort::by_id(&mut conn, &path.id).await?;
    Ok(GetAccountResponse {
        id: account.id,
        name: account.name,
        phone: account.phone,
        privileged: account.privileged,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests;
    use appctx::testing;
    use migration::run_migrations;

    #[sqlx::test]
    async fn test_get_success(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool).await;
        let account_id = tests::insert_test_account(&state.pg_pool, "13900001901").await;
        let response = execute(&state.pg_pool, GetAccountPath { id: account_id })
            .await
            .unwrap();
        assert_eq!(response.id, account_id);
        assert!(response.name.starts_with("test-"));
        assert_eq!(&*response.phone, "13900001901");
        assert!(!response.privileged);
    }

    #[sqlx::test]
    async fn test_get_not_found(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool).await;
        let err = execute(
            &state.pg_pool,
            GetAccountPath {
                id: ID::from(999_i64),
            },
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("account_not_found"));
    }
}

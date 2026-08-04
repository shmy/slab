use crate::repository::account_repository::AccountRepository;
use axum::extract::State;
use db::PgPool;
use identity_contract::port::AccountPort;
use serde::{Deserialize, Serialize};
use shared_contract::value_object::id::ID;
use shared_contract::value_object::phone_number::PhoneNumber;
use utoipa::{IntoParams, ToSchema};
use validify::Validify;
use web::extract::valid_json::ValidJson;
use web::extract::valid_path::ValidPath;
use web::response::json_response::{JsonResponse, JsonResponseType};

#[derive(Debug, Deserialize, Validify, IntoParams)]
#[into_params(parameter_in = Path)]
pub(crate) struct UpdateAccountPath {
    pub id: ID,
}

#[derive(Debug, Deserialize, Validify, ToSchema)]
pub(crate) struct UpdateAccountRequest {
    #[schema(example = "Tom")]
    pub name: String,
    #[validify]
    pub phone: PhoneNumber,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct UpdateAccountResponse {
    pub id: ID,
    #[schema(example = "Tom")]
    pub name: String,
    pub phone: PhoneNumber,
}

#[utoipa::path(
    patch,
    path = "/api/v1/accounts/{id}",
    operation_id = "account_update",
    tag = "account",
    params(UpdateAccountPath),
    request_body = UpdateAccountRequest,
    responses((status = 200, body = JsonResponse<UpdateAccountResponse>)),
    security(("bearerAuth" = []))
)]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ValidPath(path): ValidPath<UpdateAccountPath>,
    ValidJson(request): ValidJson<UpdateAccountRequest>,
) -> JsonResponseType<UpdateAccountResponse> {
    let response = execute(&pg_pool, path, request).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip(pg_pool))]
#[inline]
async fn execute(
    pg_pool: &PgPool,
    path: UpdateAccountPath,
    request: UpdateAccountRequest,
) -> rootcause::Result<UpdateAccountResponse> {
    let mut txn = pg_pool.begin().await?;
    let mut account = AccountPort::by_id(&mut txn, &path.id).await?;
    account.name = request.name;
    account.phone = request.phone;
    let account = AccountRepository::update(&mut txn, &account).await?;
    txn.commit().await?;
    Ok(UpdateAccountResponse {
        id: account.id,
        name: account.name,
        phone: account.phone,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repository::account_repository::AccountRepository;
    use crate::tests;
    use appctx::testing;
    use identity_contract::port::AccountPort;
    use migration::run_migrations;

    #[sqlx::test]
    async fn test_update_success(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool).await;
        let account_id = tests::insert_test_account(&state.pg_pool, "13900002101").await;
        let response = execute(
            &state.pg_pool,
            UpdateAccountPath { id: account_id },
            UpdateAccountRequest {
                name: "UpdatedName".to_string(),
                phone: PhoneNumber::try_new("13900002102").unwrap(),
            },
        )
        .await
        .unwrap();
        assert_eq!(response.id, account_id);
        assert_eq!(response.name, "UpdatedName");
        assert_eq!(&*response.phone, "13900002102");
    }

    #[sqlx::test]
    async fn test_update_version_conflict(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool).await;
        let account_id = tests::insert_test_account(&state.pg_pool, "13900002104").await;

        // 读到当前 account（version = 1）
        let mut conn = state.pg_pool.acquire().await.unwrap();
        let mut stale = AccountPort::by_id(&mut conn, &account_id).await.unwrap();
        stale.name = "Conflict".to_string();
        stale.phone = PhoneNumber::try_new("13900002105").unwrap();

        // 模拟并发：在读到 account 后、更新前，另一个进程改了版本
        sqlx::query!(
            r#"UPDATE accounts SET version = version + 1 WHERE id = $1"#,
            &*account_id
        )
        .execute(&mut *conn)
        .await
        .unwrap();

        // 现在 stale 的 version=1 而 DB 里已经是 2，乐观锁应拒绝
        let err = AccountRepository::update(&mut conn, &stale)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("account_version_conflict"));
    }

    #[sqlx::test]
    async fn test_update_not_found(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool).await;
        let err = execute(
            &state.pg_pool,
            UpdateAccountPath {
                id: ID::from(999_i64),
            },
            UpdateAccountRequest {
                name: "N".to_string(),
                phone: PhoneNumber::try_new("13900002103").unwrap(),
            },
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("account_not_found"));
    }
}

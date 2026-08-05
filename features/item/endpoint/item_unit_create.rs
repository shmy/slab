use audit_contract::AuditService;
use axum::extract::State;
use db::PgPool;
use http_auth::extract::operator::OperatorContext;
use item_contract::entity::ItemUnit;
use serde::{Deserialize, Serialize};
use shared_contract::value_object::id::ID;
use sqlx::Acquire;
use utoipa::{IntoParams, ToSchema};
use validify::Validify;
use web::extract::{valid_json::ValidJson, valid_path::ValidPath};
use web::response::json_response::{JsonResponse, JsonResponseType};

use crate::repository::item_unit_repository::ItemUnitRepository;

#[derive(Debug, Deserialize, Validify, IntoParams)]
#[into_params(parameter_in = Path)]
pub(crate) struct CreateUnitPath {
    pub item_id: ID,
}

#[derive(Debug, Deserialize, Validify, ToSchema)]
pub(crate) struct CreateUnitRequest {
    pub unit: String,
    pub rate: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct CreateUnitResponse {
    pub id: ID,
}

#[utoipa::path(
    post,
    path = "/api/v1/items/{item_id}/units",
    operation_id = "item_unit_create",
    tag = "item-unit",
    params(CreateUnitPath),
    request_body = CreateUnitRequest,
    responses((status = 200, body = JsonResponse<CreateUnitResponse>)),
    security(("bearerAuth" = []))
)]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ctx: OperatorContext,
    ValidPath(path): ValidPath<CreateUnitPath>,
    ValidJson(request): ValidJson<CreateUnitRequest>,
) -> JsonResponseType<CreateUnitResponse> {
    let response = execute(&pg_pool, ctx, path, request).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip(pg_pool))]
#[inline]
async fn execute(
    pg_pool: &PgPool,
    ctx: OperatorContext,
    path: CreateUnitPath,
    request: CreateUnitRequest,
) -> rootcause::Result<CreateUnitResponse> {
    let id = ID::new();
    let unit = ItemUnit {
        id,
        item_id: path.item_id,
        unit: request.unit,
        rate: request.rate,
    };
    let mut conn = pg_pool.acquire().await?;
    let mut txn = conn.begin().await?;
    ItemUnitRepository::create(&mut txn, &unit).await?;
    AuditService::record_create(&mut txn, "item_unit", &id, &ctx, &unit).await?;
    txn.commit().await?;
    Ok(CreateUnitResponse { id })
}

#[cfg(test)]
mod tests {
    use crate::tests;

    use super::*;
    use appctx::testing;
    use migration::run_migrations;

    #[sqlx::test]
    async fn test_create_success(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool).await;

        // seed item
        let item_id = ID::new();
        let mut conn = state.pg_pool.acquire().await.unwrap();
        sqlx::query!(
            r#"INSERT INTO items (id, code, name, item_type, base_unit, version)
               VALUES ($1, 'RAW-000003', '带单位物料', 1, 'kg', 1)"#,
            &*item_id
        )
        .execute(&mut *conn)
        .await
        .unwrap();
        drop(conn);

        let response = execute(
            &state.pg_pool,
            tests::test_operator_context(),
            CreateUnitPath { item_id },
            CreateUnitRequest {
                unit: "吨".into(),
                rate: 1_000_000,
            },
        )
        .await
        .unwrap();
        assert!(i64::from(response.id) > 0);

        // 变更历史：create 类型
        let mut conn = state.pg_pool.acquire().await.unwrap();
        let audit_row = sqlx::query!(
            r#"SELECT action, before, after FROM audit_logs WHERE entity_id = $1"#,
            *response.id
        )
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        assert_eq!(audit_row.action, 1); // Created
        assert!(audit_row.before.is_none());
        let after: serde_json::Value = audit_row.after.unwrap();
        assert_eq!(after["unit"], "吨");
    }
}

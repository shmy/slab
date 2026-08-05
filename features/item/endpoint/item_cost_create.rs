use audit_contract::AuditService;
use axum::extract::State;
use db::PgPool;
use http_auth::extract::operator::OperatorContext;
use item_contract::entity::{CostType, ItemCost};
use serde::{Deserialize, Serialize};
use shared_contract::value_object::id::ID;
use sqlx::Acquire;
use utoipa::{IntoParams, ToSchema};
use validify::Validify;
use web::extract::{valid_json::ValidJson, valid_path::ValidPath};
use web::response::json_response::{JsonResponse, JsonResponseType};

use crate::repository::item_cost_repository::ItemCostRepository;

#[derive(Debug, Deserialize, Validify, IntoParams)]
#[into_params(parameter_in = Path)]
pub(crate) struct CreateCostPath {
    pub item_id: ID,
}

#[derive(Debug, Deserialize, Validify, ToSchema)]
pub(crate) struct CreateCostRequest {
    pub cost_type: CostType,
    pub unit_cost: i64,
    pub currency: Option<String>,
    pub is_current: Option<bool>,
    pub remark: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct CreateCostResponse {
    pub id: ID,
}

#[utoipa::path(
    post,
    path = "/api/v1/items/{item_id}/costs",
    operation_id = "item_cost_create",
    tag = "item-cost",
    params(CreateCostPath),
    request_body = CreateCostRequest,
    responses((status = 200, body = JsonResponse<CreateCostResponse>)),
    security(("bearerAuth" = []))
)]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ctx: OperatorContext,
    ValidPath(path): ValidPath<CreateCostPath>,
    ValidJson(request): ValidJson<CreateCostRequest>,
) -> JsonResponseType<CreateCostResponse> {
    let response = execute(&pg_pool, ctx, path, request).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip_all)]
#[inline]
async fn execute(
    pg_pool: &PgPool,
    ctx: OperatorContext,
    path: CreateCostPath,
    request: CreateCostRequest,
) -> rootcause::Result<CreateCostResponse> {
    let id = ID::new();
    let cost = ItemCost {
        id,
        item_id: path.item_id,
        cost_type: request.cost_type,
        unit_cost: request.unit_cost,
        currency: request.currency.unwrap_or("CNY".into()),
        effective_at: chrono::Utc::now(),
        is_current: request.is_current.unwrap_or(true),
        remark: request.remark,
    };
    let mut conn = pg_pool.acquire().await?;
    let mut txn = conn.begin().await?;
    ItemCostRepository::create(&mut txn, &cost).await?;
    AuditService::record_create(&mut txn, "item_cost", &id, &ctx, &cost).await?;
    txn.commit().await?;
    Ok(CreateCostResponse { id })
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
               VALUES ($1, 'RAW-000004', '带成本物料', 1, 'kg', 1)"#,
            &*item_id
        )
        .execute(&mut *conn)
        .await
        .unwrap();
        drop(conn);

        let response = execute(
            &state.pg_pool,
            tests::test_operator_context(),
            CreateCostPath { item_id },
            CreateCostRequest {
                cost_type: CostType::Manual,
                unit_cost: 500,
                currency: Some("CNY".into()),
                is_current: Some(true),
                remark: None,
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
        assert_eq!(after["unit_cost"], 500);
    }
}

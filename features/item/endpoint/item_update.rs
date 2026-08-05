use audit_contract::AuditService;
use axum::extract::State;
use db::PgPool;
use http_auth::extract::operator::OperatorContext;
use item_contract::port::ItemPort;
use serde::{Deserialize, Serialize};
use shared_contract::value_object::id::ID;
use sqlx::Acquire;
use utoipa::{IntoParams, ToSchema};
use validify::Validify;
use web::extract::{valid_json::ValidJson, valid_path::ValidPath};
use web::response::json_response::{JsonResponse, JsonResponseType};

use crate::repository::item_repository::ItemRepository;

#[derive(Debug, Deserialize, Validify, IntoParams)]
#[into_params(parameter_in = Path)]
pub(crate) struct UpdateItemPath {
    pub id: ID,
}

#[derive(Debug, Deserialize, Validify, ToSchema)]
pub(crate) struct UpdateItemRequest {
    pub name: Option<String>,
    pub category_id: Option<ID>,
    pub base_unit: Option<String>,
    pub parent_item_id: Option<Option<ID>>,
    pub spec: Option<Option<String>>,
    pub is_active: Option<bool>,
    pub reorder_point: Option<i64>,
    pub safety_stock: Option<i64>,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct UpdateItemResponse {
    pub updated: bool,
}

#[utoipa::path(
    patch,
    path = "/api/v1/items/{id}",
    operation_id = "item_update",
    tag = "item",
    params(UpdateItemPath),
    request_body = UpdateItemRequest,
    responses((status = 200, body = JsonResponse<UpdateItemResponse>)),
    security(("bearerAuth" = []))
)]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ctx: OperatorContext,
    ValidPath(path): ValidPath<UpdateItemPath>,
    ValidJson(request): ValidJson<UpdateItemRequest>,
) -> JsonResponseType<UpdateItemResponse> {
    let response = execute(&pg_pool, ctx, path, request).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip_all)]
#[inline]
async fn execute(
    pg_pool: &PgPool,
    ctx: OperatorContext,
    path: UpdateItemPath,
    request: UpdateItemRequest,
) -> rootcause::Result<UpdateItemResponse> {
    let mut conn = pg_pool.acquire().await?;
    let mut txn = conn.begin().await?;
    // 更新前同事务读当前行作为变更历史 before
    let before = ItemPort::by_id(&mut txn, &path.id).await?;
    let updated = ItemRepository::update(&mut txn, &path.id, &request).await?;
    // 更新后同事务重读（可见自身未提交写入）作为 after
    let after = ItemPort::by_id(&mut txn, &path.id).await?;
    if let (Some(before), Some(after)) = (before, after) {
        AuditService::record_updated(&mut txn, "item", &path.id, &ctx, &before, &after).await?;
    }
    txn.commit().await?;
    Ok(UpdateItemResponse { updated })
}

#[cfg(test)]
mod tests {
    use crate::tests;

    use super::*;
    use appctx::testing;
    use migration::run_migrations;

    #[sqlx::test]
    async fn test_update_success(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool).await;

        // seed item
        let item_id = ID::new();
        let mut conn = state.pg_pool.acquire().await.unwrap();
        sqlx::query!(
            r#"INSERT INTO items (id, code, name, item_type, base_unit, version)
               VALUES ($1, 'RAW-000001', '原材料', 1, 'kg', 1)"#,
            &*item_id
        )
        .execute(&mut *conn)
        .await
        .unwrap();
        drop(conn);

        let response = execute(
            &state.pg_pool,
            tests::test_operator_context(),
            UpdateItemPath { id: item_id },
            UpdateItemRequest {
                name: Some("更新后名称".into()),
                category_id: None,
                base_unit: None,
                parent_item_id: None,
                spec: None,
                is_active: None,
                reorder_point: None,
                safety_stock: None,
            },
        )
        .await
        .unwrap();
        assert!(response.updated);

        // 变更历史：update 类型，before/after 快照
        let mut conn = state.pg_pool.acquire().await.unwrap();
        let audit_row = sqlx::query!(
            r#"SELECT action, before, after FROM audit_logs WHERE entity_id = $1"#,
            *item_id
        )
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        assert_eq!(audit_row.action, 2); // Updated
        let before: serde_json::Value = audit_row.before.unwrap();
        let after: serde_json::Value = audit_row.after.unwrap();
        assert_eq!(before["name"], "原材料");
        assert_eq!(after["name"], "更新后名称");
    }
}

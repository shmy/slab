use audit_contract::AuditService;
use axum::extract::State;
use db::PgPool;
use http_auth::extract::operator::OperatorContext;
use item_contract::entity::ItemCategory;
use serde::{Deserialize, Serialize};
use shared_contract::value_object::id::ID;
use sqlx::Acquire;
use utoipa::{IntoParams, ToSchema};
use validify::Validify;
use web::extract::{valid_json::ValidJson, valid_path::ValidPath};
use web::response::json_response::{JsonResponse, JsonResponseType};

use crate::repository::item_category_repository::ItemCategoryRepository;

#[derive(Debug, Deserialize, Validify, IntoParams)]
#[into_params(parameter_in = Path)]
pub(crate) struct UpdateCategoryPath {
    pub id: ID,
}

#[derive(Debug, Deserialize, Validify, ToSchema)]
pub(crate) struct UpdateCategoryRequest {
    pub name: Option<String>,
    pub parent_id: Option<Option<ID>>,
    pub sort_order: Option<i32>,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct UpdateCategoryResponse {
    pub updated: bool,
}

#[utoipa::path(
    patch,
    path = "/api/v1/item-categories/{id}",
    operation_id = "item_category_update",
    tag = "item-category",
    params(UpdateCategoryPath),
    request_body = UpdateCategoryRequest,
    responses((status = 200, body = JsonResponse<UpdateCategoryResponse>)),
    security(("bearerAuth" = []))
)]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ctx: OperatorContext,
    ValidPath(path): ValidPath<UpdateCategoryPath>,
    ValidJson(request): ValidJson<UpdateCategoryRequest>,
) -> JsonResponseType<UpdateCategoryResponse> {
    let response = execute(&pg_pool, ctx, path, request).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip_all)]
#[inline]
async fn execute(
    pg_pool: &PgPool,
    ctx: OperatorContext,
    path: UpdateCategoryPath,
    request: UpdateCategoryRequest,
) -> rootcause::Result<UpdateCategoryResponse> {
    let mut conn = pg_pool.acquire().await?;
    let mut txn = conn.begin().await?;
    // 更新前同事务读当前行作为变更历史 before
    let before = read_category(&mut txn, &path.id).await?;
    let updated = ItemCategoryRepository::update(&mut txn, &path.id, &request).await?;
    // 更新后同事务重读（可见自身未提交写入）作为 after
    let after = read_category(&mut txn, &path.id).await?;
    if let (Some(before), Some(after)) = (before, after) {
        AuditService::record_updated(&mut txn, "item_category", &path.id, &ctx, &before, &after)
            .await?;
    }
    txn.commit().await?;
    Ok(UpdateCategoryResponse { updated })
}

/// 同事务读分类行，映射为实体；行不存在返回 `None`。
async fn read_category(
    conn: &mut sqlx::PgConnection,
    id: &ID,
) -> rootcause::Result<Option<ItemCategory>> {
    let row = sqlx::query!(
        r#"SELECT id, name, parent_id, sort_order, is_active
           FROM item_categories WHERE id = $1"#,
        id as _
    )
    .fetch_optional(&mut *conn)
    .await?;
    Ok(row.map(|r| ItemCategory {
        id: ID::new_unchecked(r.id),
        name: r.name,
        parent_id: r.parent_id.map(ID::new_unchecked),
        sort_order: r.sort_order,
        is_active: r.is_active,
    }))
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

        // seed category
        let cat_id = ID::new();
        let mut conn = state.pg_pool.acquire().await.unwrap();
        sqlx::query!(
            "INSERT INTO item_categories (id, name, sort_order) VALUES ($1, '旧分类', 1)",
            &*cat_id
        )
        .execute(&mut *conn)
        .await
        .unwrap();
        drop(conn);

        let response = execute(
            &state.pg_pool,
            tests::test_operator_context(),
            UpdateCategoryPath { id: cat_id },
            UpdateCategoryRequest {
                name: Some("新分类".into()),
                parent_id: None,
                sort_order: None,
            },
        )
        .await
        .unwrap();
        assert!(response.updated);

        // 变更历史：update 类型，before/after 快照
        let mut conn = state.pg_pool.acquire().await.unwrap();
        let audit_row = sqlx::query!(
            r#"SELECT action, before, after FROM audit_logs WHERE entity_id = $1"#,
            *cat_id
        )
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        assert_eq!(audit_row.action, 2); // Updated
        let before: serde_json::Value = audit_row.before.unwrap();
        let after: serde_json::Value = audit_row.after.unwrap();
        assert_eq!(before["name"], "旧分类");
        assert_eq!(after["name"], "新分类");
    }
}

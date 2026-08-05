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
use web::extract::valid_path::ValidPath;
use web::response::json_response::{JsonResponse, JsonResponseType};

use crate::repository::item_category_repository::ItemCategoryRepository;

#[derive(Debug, Deserialize, Validify, IntoParams)]
#[into_params(parameter_in = Path)]
pub(crate) struct DeleteCategoryPath {
    pub id: ID,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct DeleteCategoryResponse {
    pub deleted: bool,
}

#[utoipa::path(
    delete,
    path = "/api/v1/item-categories/{id}",
    operation_id = "item_category_delete",
    tag = "item-category",
    params(DeleteCategoryPath),
    responses((status = 200, body = JsonResponse<DeleteCategoryResponse>)),
    security(("bearerAuth" = []))
)]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ctx: OperatorContext,
    ValidPath(path): ValidPath<DeleteCategoryPath>,
) -> JsonResponseType<DeleteCategoryResponse> {
    let response = execute(&pg_pool, ctx, path).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip(pg_pool))]
#[inline]
async fn execute(
    pg_pool: &PgPool,
    ctx: OperatorContext,
    path: DeleteCategoryPath,
) -> rootcause::Result<DeleteCategoryResponse> {
    let mut conn = pg_pool.acquire().await?;
    let mut txn = conn.begin().await?;
    // 删除前读旧值用于变更历史；行不存在时不产生记录（幂等）
    let before = read_category(&mut txn, &path.id).await?;
    let deleted = ItemCategoryRepository::delete(&mut txn, &path.id).await?;
    if let Some(before) = before {
        AuditService::record_deleted(&mut txn, "item_category", &path.id, &ctx, &before).await?;
    }
    txn.commit().await?;
    Ok(DeleteCategoryResponse { deleted })
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
    async fn test_delete_success(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool).await;

        // seed category
        let cat_id = ID::new();
        let mut conn = state.pg_pool.acquire().await.unwrap();
        sqlx::query!(
            "INSERT INTO item_categories (id, name, sort_order) VALUES ($1, '待删分类', 1)",
            &*cat_id
        )
        .execute(&mut *conn)
        .await
        .unwrap();
        drop(conn);

        let response = execute(
            &state.pg_pool,
            tests::test_operator_context(),
            DeleteCategoryPath { id: cat_id },
        )
        .await
        .unwrap();
        assert!(response.deleted);

        // 变更历史：delete 类型，after 为空
        let mut conn = state.pg_pool.acquire().await.unwrap();
        let audit_row = sqlx::query!(
            r#"SELECT action, before, after FROM audit_logs WHERE entity_id = $1"#,
            *cat_id
        )
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        assert_eq!(audit_row.action, 3); // Deleted
        let before: serde_json::Value = audit_row.before.unwrap();
        assert_eq!(before["name"], "待删分类");
        assert!(audit_row.after.is_none());
    }

    #[sqlx::test]
    async fn test_delete_nonexistent_no_audit(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool).await;
        let response = execute(
            &state.pg_pool,
            tests::test_operator_context(),
            DeleteCategoryPath {
                id: ID::from(999_i64),
            },
        )
        .await
        .unwrap();
        assert!(!response.deleted);

        // 不存在的行：不产生变更记录
        let mut conn = state.pg_pool.acquire().await.unwrap();
        let count = sqlx::query!(
            r#"SELECT COUNT(*) AS "count!" FROM audit_logs WHERE entity_id = $1"#,
            999i64
        )
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        assert_eq!(count.count, 0);
    }
}

use audit_contract::AuditService;
use axum::extract::State;
use db::PgPool;
use http_auth::extract::operator::OperatorContext;
use item_contract::entity::ItemCategory;
use serde::{Deserialize, Serialize};
use shared_contract::value_object::id::ID;
use sqlx::Acquire;
use utoipa::ToSchema;
use validify::Validify;
use web::extract::valid_json::ValidJson;
use web::response::json_response::{JsonResponse, JsonResponseType};

use crate::repository::item_category_repository::ItemCategoryRepository;

#[derive(Debug, Deserialize, Validify, ToSchema)]
pub(crate) struct CreateCategoryRequest {
    pub name: String,
    pub parent_id: Option<ID>,
    pub sort_order: Option<i32>,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct CreateCategoryResponse {
    pub id: ID,
}

#[utoipa::path(
    post,
    path = "/api/v1/item-categories",
    operation_id = "item_category_create",
    tag = "item-category",
    request_body = CreateCategoryRequest,
    responses((status = 200, body = JsonResponse<CreateCategoryResponse>)),
    security(("bearerAuth" = []))
)]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ctx: OperatorContext,
    ValidJson(request): ValidJson<CreateCategoryRequest>,
) -> JsonResponseType<CreateCategoryResponse> {
    let response = execute(&pg_pool, ctx, request).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip(pg_pool))]
#[inline]
async fn execute(
    pg_pool: &PgPool,
    ctx: OperatorContext,
    request: CreateCategoryRequest,
) -> rootcause::Result<CreateCategoryResponse> {
    let id = ID::new();
    let category = ItemCategory {
        id,
        name: request.name,
        parent_id: request.parent_id,
        sort_order: request.sort_order.unwrap_or(0),
        is_active: true,
    };
    let mut conn = pg_pool.acquire().await?;
    let mut txn = conn.begin().await?;
    ItemCategoryRepository::create(&mut txn, &category).await?;
    AuditService::record_create(&mut txn, "item_category", &id, &ctx, &category).await?;
    txn.commit().await?;
    Ok(CreateCategoryResponse { id })
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

        let request = CreateCategoryRequest {
            name: "塑料".into(),
            parent_id: None,
            sort_order: Some(1),
        };
        let response = execute(&state.pg_pool, tests::test_operator_context(), request)
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
        assert_eq!(after["name"], "塑料");
    }
}

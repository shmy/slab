use axum::extract::State;
use db::PgPool;
use item_contract::entity::{Item, ItemType};
use serde::{Deserialize, Serialize};
use shared_contract::value_object::id::ID;
use sqlx::Acquire;
use utoipa::ToSchema;
use validify::Validify;
use web::extract::valid_json::ValidJson;
use web::response::json_response::{JsonResponse, JsonResponseType};

use crate::repository::item_repository::ItemRepository;

#[derive(Debug, Deserialize, Validify, ToSchema)]
pub(crate) struct CreateItemRequest {
    #[schema(example = "ABS 塑料米")]
    pub name: String,
    pub category_id: ID,
    pub item_type: ItemType,
    #[schema(example = "kg")]
    pub base_unit: String,
    pub parent_item_id: Option<ID>,
    pub spec: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct CreateItemResponse {
    pub id: ID,
    pub code: String,
}

#[utoipa::path(
    post,
    path = "/api/v1/items",
    operation_id = "item_create",
    tag = "item",
    request_body = CreateItemRequest,
    responses((status = 200, body = JsonResponse<CreateItemResponse>)),
    security(("bearerAuth" = []))
)]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ValidJson(request): ValidJson<CreateItemRequest>,
) -> JsonResponseType<CreateItemResponse> {
    let response = execute(&pg_pool, request).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip_all)]
#[inline]
async fn execute(
    pg_pool: &PgPool,
    request: CreateItemRequest,
) -> rootcause::Result<CreateItemResponse> {
    let id = ID::new();
    let mut conn = pg_pool.acquire().await?;
    let mut txn = conn.begin().await?;

    let code = ItemRepository::generate_code(&mut txn, request.item_type).await?;
    let item = Item {
        id,
        code: code.clone(),
        name: request.name,
        category_id: request.category_id,
        item_type: request.item_type,
        base_unit: request.base_unit,
        parent_item_id: request.parent_item_id,
        spec: request.spec,
        is_active: true,
        reorder_point: 0,
        safety_stock: 0,
        version: 1,
    };
    ItemRepository::create(&mut txn, &item).await?;
    txn.commit().await?;
    Ok(CreateItemResponse { id, code })
}

#[cfg(test)]
mod tests {
    use super::*;
    use appctx::testing;
    use migration::run_migrations;

    #[sqlx::test]
    async fn test_create_success(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool).await;

        // seed category
        let cat_id = ID::new();
        let mut conn = state.pg_pool.acquire().await.unwrap();
        sqlx::query!(
            "INSERT INTO item_categories (id, name) VALUES ($1, '塑料')",
            &*cat_id
        )
        .execute(&mut *conn)
        .await
        .unwrap();
        drop(conn);

        let request = CreateItemRequest {
            name: "ABS 塑料米".into(),
            category_id: cat_id,
            item_type: ItemType::RawMaterial,
            base_unit: "kg".into(),
            parent_item_id: None,
            spec: None,
        };
        let response = execute(&state.pg_pool, request).await.unwrap();
        assert!(i64::from(response.id) > 0);
        assert!(response.code.starts_with("RAW-"));
    }
}

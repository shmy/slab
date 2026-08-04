use axum::extract::State;
use db::PgPool;
use item_contract::error::ItemError;
use serde::{Deserialize, Serialize};
use shared_contract::value_object::id::ID;
use utoipa::{IntoParams, ToSchema};
use validify::Validify;
use web::extract::valid_path::ValidPath;
use web::response::json_response::{JsonResponse, JsonResponseType};

#[derive(Debug, Deserialize, Validify, IntoParams)]
#[into_params(parameter_in = Path)]
pub(crate) struct WeightedCostPath {
    pub id: ID,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct WeightedCostResponse {
    pub item_id: ID,
    pub unit_cost: i64,
    pub currency: String,
}

#[utoipa::path(
    get, path = "/api/v1/items/{id}/weighted-cost",
    operation_id = "item_weighted_cost_get", tag = "item",
    params(WeightedCostPath),
    responses((status = 200, body = JsonResponse<WeightedCostResponse>)),
    security(("bearerAuth" = []))
)]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ValidPath(path): ValidPath<WeightedCostPath>,
) -> JsonResponseType<WeightedCostResponse> {
    let response = execute(&pg_pool, path).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip_all)]
#[inline]
async fn execute(
    pg_pool: &PgPool,
    path: WeightedCostPath,
) -> rootcause::Result<WeightedCostResponse> {
    let mut conn = pg_pool.acquire().await?;
    let row = sqlx::query!(
        r#"SELECT unit_cost, currency FROM item_costs
           WHERE item_id = $1 AND cost_type = 10 AND is_current = TRUE"#,
        &*path.id
    )
    .fetch_optional(&mut *conn)
    .await?
    .ok_or(ItemError::NotFound)?;

    Ok(WeightedCostResponse {
        item_id: path.id,
        unit_cost: row.unit_cost,
        currency: row.currency.unwrap_or_else(|| "CNY".to_string()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use appctx::testing;
    use migration::run_migrations;

    #[sqlx::test]
    async fn test_get_weighted_cost(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let _state = testing::build(pool.clone()).await;
        let mut conn = pool.acquire().await.unwrap();

        let cat_id = ID::new();
        let item_id = ID::new();
        sqlx::query!(
            "INSERT INTO item_categories (id, name) VALUES ($1, 'Test')",
            &*cat_id
        )
        .execute(&mut *conn)
        .await
        .unwrap();
        sqlx::query!("INSERT INTO items (id, code, name, category_id, item_type, base_unit) VALUES ($1, 'IT001', 'TestItem', $2, 1, 'kg')",
            &*item_id, &*cat_id).execute(&mut *conn).await.unwrap();

        let cost_id = ID::new();
        sqlx::query!(
            "INSERT INTO item_costs (id, item_id, cost_type, unit_cost, currency, effective_at, is_current) VALUES ($1, $2, 10, 3500, 'CNY', NOW(), true)",
            &*cost_id, &*item_id
        ).execute(&mut *conn).await.unwrap();

        let resp = execute(&pool, WeightedCostPath { id: item_id })
            .await
            .unwrap();
        assert_eq!(resp.unit_cost, 3500);
        assert_eq!(resp.currency, "CNY");
    }

    #[sqlx::test]
    async fn test_weighted_cost_not_found(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let _state = testing::build(pool.clone()).await;
        let err = execute(&pool, WeightedCostPath { id: ID::new() })
            .await
            .unwrap_err();
        assert!(err.to_string().contains("item_not_found"));
    }
}

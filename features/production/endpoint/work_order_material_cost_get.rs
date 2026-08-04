use axum::extract::State;
use db::PgPool;
use production_contract::error::ProductionError;
use serde::{Deserialize, Serialize};
use shared_contract::value_object::id::ID;
use utoipa::{IntoParams, ToSchema};
use validify::Validify;
use web::extract::valid_path::ValidPath;
use web::response::json_response::{JsonResponse, JsonResponseType};

#[derive(Debug, Deserialize, Validify, IntoParams)]
#[into_params(parameter_in = Path)]
pub(crate) struct MaterialCostPath {
    pub id: ID,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct MaterialCostLine {
    pub item_id: ID,
    pub item_code: String,
    pub item_name: String,
    pub picked_qty: i64,
    pub unit_cost: i64,
    pub line_cost: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct MaterialCostResponse {
    pub work_order_id: ID,
    pub lines: Vec<MaterialCostLine>,
    pub total_material_cost: i64,
}

#[utoipa::path(
    get, path = "/api/v1/production/work-orders/{id}/material-cost",
    operation_id = "work_order_material_cost_get", tag = "production",
    params(MaterialCostPath),
    responses((status = 200, body = JsonResponse<MaterialCostResponse>)),
    security(("bearerAuth" = []))
)]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ValidPath(path): ValidPath<MaterialCostPath>,
) -> JsonResponseType<MaterialCostResponse> {
    let response = execute(&pg_pool, path).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip_all)]
#[inline]
async fn execute(
    pg_pool: &PgPool,
    path: MaterialCostPath,
) -> rootcause::Result<MaterialCostResponse> {
    let mut conn = pg_pool.acquire().await?;

    // 验证工单存在
    sqlx::query!("SELECT id FROM work_orders WHERE id = $1", &*path.id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or(ProductionError::NotFound)?;

    let rows = sqlx::query!(
        r#"SELECT wm.item_id, i.code, i.name, wm.picked_qty,
                  COALESCE(ic.unit_cost, 0)::BIGINT AS "unit_cost!"
           FROM work_order_materials wm
           JOIN items i ON i.id = wm.item_id
           LEFT JOIN item_costs ic ON ic.item_id = wm.item_id
               AND ic.cost_type = 10 AND ic.is_current = TRUE
           WHERE wm.work_order_id = $1"#,
        &*path.id
    )
    .fetch_all(&mut *conn)
    .await?;

    let lines: Vec<MaterialCostLine> = rows
        .into_iter()
        .map(|r| MaterialCostLine {
            item_id: ID::new_unchecked(r.item_id),
            item_code: r.code,
            item_name: r.name,
            picked_qty: r.picked_qty.unwrap_or(0),
            unit_cost: r.unit_cost,
            line_cost: r.picked_qty.unwrap_or(0) * r.unit_cost,
        })
        .collect();

    let total_material_cost: i64 = lines.iter().map(|l| l.line_cost).sum();

    Ok(MaterialCostResponse {
        work_order_id: path.id,
        lines,
        total_material_cost,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use appctx::testing;
    use migration::run_migrations;

    #[sqlx::test]
    async fn test_material_cost(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let _state = testing::build(pool.clone()).await;
        let mut conn = pool.acquire().await.unwrap();

        let cat_id = ID::new();
        let item_id = ID::new();
        let wo_id = ID::new();
        sqlx::query!(
            "INSERT INTO item_categories (id, name) VALUES ($1, 'Test')",
            &*cat_id
        )
        .execute(&mut *conn)
        .await
        .unwrap();
        sqlx::query!("INSERT INTO items (id, code, name, category_id, item_type, base_unit) VALUES ($1, 'IT001', 'TestItem', $2, 1, 'kg')",
            &*item_id, &*cat_id).execute(&mut *conn).await.unwrap();

        let bom_id = ID::new();
        sqlx::query!("INSERT INTO boms (id, code, name, item_id, status) VALUES ($1, 'BOM001', 'TestBOM', $2, 1)",
            &*bom_id, &*item_id).execute(&mut *conn).await.unwrap();
        sqlx::query!("INSERT INTO work_orders (id, code, bom_id, item_id, planned_qty, status) VALUES ($1, 'WO001', $2, $3, 100, 0)",
            &*wo_id, &*bom_id, &*item_id).execute(&mut *conn).await.unwrap();

        let wm_id = ID::new();
        sqlx::query!("INSERT INTO work_order_materials (id, work_order_id, item_id, required_qty, picked_qty) VALUES ($1, $2, $3, 200, 150)",
            &*wm_id, &*wo_id, &*item_id).execute(&mut *conn).await.unwrap();

        let cost_id = ID::new();
        sqlx::query!("INSERT INTO item_costs (id, item_id, cost_type, unit_cost, currency, effective_at, is_current) VALUES ($1, $2, 10, 2000, 'CNY', NOW(), true)",
            &*cost_id, &*item_id).execute(&mut *conn).await.unwrap();

        let resp = execute(&pool, MaterialCostPath { id: wo_id })
            .await
            .unwrap();
        assert_eq!(resp.lines.len(), 1);
        assert_eq!(resp.lines[0].picked_qty, 150);
        assert_eq!(resp.lines[0].unit_cost, 2000);
        assert_eq!(resp.lines[0].line_cost, 300000);
        assert_eq!(resp.total_material_cost, 300000);
    }
}

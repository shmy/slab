use axum::extract::State;
use db::PgPool;
use planning_contract::port::PlanningPort;
use serde::Serialize;
use shared_contract::value_object::id::ID;
use utoipa::ToSchema;
use web::response::json_response::{JsonResponse, JsonResponseType};

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct ReorderAlertItem {
    pub item_id: ID,
    pub item_code: String,
    pub item_name: String,
    /// 当前库存总量（所有仓库求和）
    pub current_stock: i64,
    /// 再订货点
    pub reorder_point: i64,
    /// 短缺量 = reorder_point - current_stock
    pub shortage: i64,
}

#[utoipa::path(
    get, path = "/api/v1/planning/reorder-alerts",
    operation_id = "planning_reorder_alerts", tag = "planning",
    responses((status = 200, body = JsonResponse<Vec<ReorderAlertItem>>)),
    security(("bearerAuth" = []))
)]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
) -> JsonResponseType<Vec<ReorderAlertItem>> {
    let response = execute(&pg_pool).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip_all)]
#[inline]
async fn execute(pg_pool: &PgPool) -> rootcause::Result<Vec<ReorderAlertItem>> {
    let mut conn = pg_pool.acquire().await?;

    // 库存聚合口径由 PlanningPort 统一提供，此处只做业务过滤与计算
    let aggs = PlanningPort::stock_and_transit(&mut conn).await?;
    let mut items: Vec<ReorderAlertItem> = aggs
        .into_iter()
        .filter(|a| a.reorder_point > 0 && a.current_stock < a.reorder_point)
        .map(|a| ReorderAlertItem {
            item_id: a.item_id,
            item_code: a.item_code,
            item_name: a.item_name,
            current_stock: a.current_stock,
            reorder_point: a.reorder_point,
            shortage: (a.reorder_point - a.current_stock).max(0),
        })
        .collect();

    items.sort_by_key(|a| std::cmp::Reverse(a.shortage));
    Ok(items)
}

#[cfg(test)]
mod tests {
    use super::*;
    use appctx::testing;
    use migration::run_migrations;

    #[sqlx::test]
    async fn test_reorder_alert(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let _state = testing::build(pool.clone()).await;
        let mut conn = pool.acquire().await.unwrap();

        let cat_id = ID::new();
        sqlx::query!(
            "INSERT INTO item_categories (id, name) VALUES ($1, 'Raw')",
            &*cat_id
        )
        .execute(&mut *conn)
        .await
        .unwrap();
        let wh_id = ID::new();
        sqlx::query!("INSERT INTO warehouses (id, code, name, type, is_active) VALUES ($1, 'WH01', 'Main', 1, true)", &*wh_id)
            .execute(&mut *conn).await.unwrap();

        let item1 = ID::new();
        sqlx::query!("INSERT INTO items (id, code, name, category_id, item_type, base_unit, reorder_point, safety_stock) VALUES ($1, 'RAW001', 'Steel', $2, 1, 'kg', 1000, 500)",
            &*item1, &*cat_id).execute(&mut *conn).await.unwrap();
        let item2 = ID::new();
        sqlx::query!("INSERT INTO items (id, code, name, category_id, item_type, base_unit, reorder_point, safety_stock) VALUES ($1, 'RAW002', 'Copper', $2, 1, 'kg', 500, 200)",
            &*item2, &*cat_id).execute(&mut *conn).await.unwrap();
        let item3 = ID::new();
        sqlx::query!("INSERT INTO items (id, code, name, category_id, item_type, base_unit, reorder_point, safety_stock) VALUES ($1, 'RAW003', 'Aluminum', $2, 1, 'kg', 300, 100)",
            &*item3, &*cat_id).execute(&mut *conn).await.unwrap();

        sqlx::query!("INSERT INTO inventories (id, item_id, warehouse_id, quantity, locked_qty, version) VALUES ($1, $2, $3, 800, 0, 1)",
            &*ID::new(), &*item1, &*wh_id).execute(&mut *conn).await.unwrap();
        sqlx::query!("INSERT INTO inventories (id, item_id, warehouse_id, quantity, locked_qty, version) VALUES ($1, $2, $3, 600, 0, 1)",
            &*ID::new(), &*item2, &*wh_id).execute(&mut *conn).await.unwrap();

        let alerts = execute(&pool).await.unwrap();
        assert_eq!(alerts.len(), 2);

        let al = alerts.iter().find(|a| a.item_code == "RAW003").unwrap();
        assert_eq!(al.shortage, 300);
        assert_eq!(al.current_stock, 0);

        let steel = alerts.iter().find(|a| a.item_code == "RAW001").unwrap();
        assert_eq!(steel.shortage, 200);
        assert_eq!(steel.current_stock, 800);
    }
}

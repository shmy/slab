use axum::extract::State;
use db::PgPool;
use item_contract::entity::ItemType;
use planning_contract::port::PlanningPort;
use serde::Serialize;
use shared_contract::value_object::id::ID;
use utoipa::ToSchema;
use web::response::json_response::{JsonResponse, JsonResponseType};

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct PurchaseSuggestionItem {
    pub item_id: ID,
    pub item_code: String,
    pub item_name: String,
    /// 当前库存总量
    pub current_stock: i64,
    /// 在途采购量（已审批未收货的采购订单行 quantity - received_qty）
    pub in_transit_qty: i64,
    /// 安全库存
    pub safety_stock: i64,
    /// 建议采购量 = safety_stock - current_stock - in_transit_qty（小于 0 则返回 0）
    pub suggested_qty: i64,
}

#[utoipa::path(
    get, path = "/api/v1/planning/purchase-suggestions",
    operation_id = "planning_purchase_suggestions", tag = "planning",
    responses((status = 200, body = JsonResponse<Vec<PurchaseSuggestionItem>>)),
    security(("bearerAuth" = []))
)]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
) -> JsonResponseType<Vec<PurchaseSuggestionItem>> {
    let response = execute(&pg_pool).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip_all)]
#[inline]
async fn execute(pg_pool: &PgPool) -> rootcause::Result<Vec<PurchaseSuggestionItem>> {
    let mut conn = pg_pool.acquire().await?;

    // 库存/在途聚合口径由 PlanningPort 统一提供，此处只做业务过滤与计算
    let aggs = PlanningPort::stock_and_transit(&mut conn).await?;
    let mut items: Vec<PurchaseSuggestionItem> = aggs
        .into_iter()
        .filter(|a| a.item_type == ItemType::Purchased as i16 && a.safety_stock > 0)
        .map(|a| {
            let suggested_qty = (a.safety_stock - a.current_stock - a.in_transit_qty).max(0);
            PurchaseSuggestionItem {
                item_id: a.item_id,
                item_code: a.item_code,
                item_name: a.item_name,
                current_stock: a.current_stock,
                in_transit_qty: a.in_transit_qty,
                safety_stock: a.safety_stock,
                suggested_qty,
            }
        })
        .filter(|item| item.suggested_qty > 0)
        .collect();

    items.sort_by_key(|item| -item.suggested_qty);

    Ok(items)
}

#[cfg(test)]
mod tests {
    use super::*;
    use appctx::testing;
    use migration::run_migrations;

    #[sqlx::test]
    async fn test_purchase_suggestion(pool: sqlx::PgPool) {
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
        let item2 = ID::new();
        sqlx::query!("INSERT INTO items (id, code, name, category_id, item_type, base_unit, reorder_point, safety_stock) VALUES ($1, 'PUR001', 'Resin', $2, 3, 'kg', 0, 500)",
            &*item1, &*cat_id).execute(&mut *conn).await.unwrap();
        sqlx::query!("INSERT INTO items (id, code, name, category_id, item_type, base_unit, reorder_point, safety_stock) VALUES ($1, 'PUR002', 'Cable', $2, 3, 'm', 0, 300)",
            &*item2, &*cat_id).execute(&mut *conn).await.unwrap();

        sqlx::query!("INSERT INTO inventories (id, item_id, warehouse_id, quantity, locked_qty, version) VALUES ($1, $2, $3, 200, 0, 1)",
            &*ID::new(), &*item1, &*wh_id).execute(&mut *conn).await.unwrap();
        sqlx::query!("INSERT INTO inventories (id, item_id, warehouse_id, quantity, locked_qty, version) VALUES ($1, $2, $3, 350, 0, 1)",
            &*ID::new(), &*item2, &*wh_id).execute(&mut *conn).await.unwrap();

        let supplier_id = ID::new();
        sqlx::query!(
            "INSERT INTO suppliers (id, code, name, is_active) VALUES ($1, 'S001', 'Test', true)",
            &*supplier_id
        )
        .execute(&mut *conn)
        .await
        .unwrap();
        let po_id = ID::new();
        sqlx::query!("INSERT INTO purchase_orders (id, code, supplier_id, status, order_date, currency, total_amount) VALUES ($1, 'PO001', $2, 3, CURRENT_DATE, 'CNY', 10000)",
            &*po_id, &*supplier_id).execute(&mut *conn).await.unwrap();
        sqlx::query!("INSERT INTO purchase_order_lines (id, order_id, item_id, quantity, unit, unit_price, line_total, received_qty) VALUES ($1, $2, $3, 100, 'kg', 50, 5000, 0)",
            &*ID::new(), &*po_id, &*item1).execute(&mut *conn).await.unwrap();

        let result = execute(&pool).await.unwrap();
        assert_eq!(result.len(), 1);
        let r = &result[0];
        assert_eq!(r.item_code, "PUR001");
        assert_eq!(r.current_stock, 200);
        assert_eq!(r.in_transit_qty, 100);
        assert_eq!(r.safety_stock, 500);
        assert_eq!(r.suggested_qty, 200);
    }

    #[sqlx::test]
    async fn test_purchase_suggestion_no_alerts(pool: sqlx::PgPool) {
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
        let item = ID::new();
        sqlx::query!("INSERT INTO items (id, code, name, category_id, item_type, base_unit, safety_stock) VALUES ($1, 'PUR003', 'Paint', $2, 3, 'kg', 0)",
            &*item, &*cat_id).execute(&mut *conn).await.unwrap();
        sqlx::query!("INSERT INTO inventories (id, item_id, warehouse_id, quantity, locked_qty, version) VALUES ($1, $2, $3, 100, 0, 1)",
            &*ID::new(), &*item, &*wh_id).execute(&mut *conn).await.unwrap();

        let result = execute(&pool).await.unwrap();
        assert_eq!(result.len(), 0);
    }
    #[sqlx::test]
    async fn test_suggestion_exact_safety_stock_no_alert(pool: sqlx::PgPool) {
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
        let item = ID::new();
        sqlx::query!("INSERT INTO items (id, code, name, category_id, item_type, base_unit, safety_stock) VALUES ($1, 'PUR004', 'Resin', $2, 3, 'kg', 500)",
            &*item, &*cat_id).execute(&mut *conn).await.unwrap();
        // 库存恰好 = 安全库存 → suggested = 0，应被过滤（等号边界）
        sqlx::query!("INSERT INTO inventories (id, item_id, warehouse_id, quantity, locked_qty, version) VALUES ($1, $2, $3, 500, 0, 1)",
            &*ID::new(), &*item, &*wh_id).execute(&mut *conn).await.unwrap();

        let result = execute(&pool).await.unwrap();
        assert_eq!(result.len(), 0);
    }

    #[sqlx::test]
    async fn test_suggestion_transit_closes_gap(pool: sqlx::PgPool) {
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
        let item = ID::new();
        sqlx::query!("INSERT INTO items (id, code, name, category_id, item_type, base_unit, safety_stock) VALUES ($1, 'PUR005', 'Resin', $2, 3, 'kg', 500)",
            &*item, &*cat_id).execute(&mut *conn).await.unwrap();
        // 库存 400 + 在途 100 恰好补足安全库存 500 → suggested = 0
        sqlx::query!("INSERT INTO inventories (id, item_id, warehouse_id, quantity, locked_qty, version) VALUES ($1, $2, $3, 400, 0, 1)",
            &*ID::new(), &*item, &*wh_id).execute(&mut *conn).await.unwrap();
        let supplier_id = ID::new();
        sqlx::query!(
            "INSERT INTO suppliers (id, code, name, is_active) VALUES ($1, 'S002', 'Test', true)",
            &*supplier_id
        )
        .execute(&mut *conn)
        .await
        .unwrap();
        let po_id = ID::new();
        sqlx::query!("INSERT INTO purchase_orders (id, code, supplier_id, status, order_date, currency, total_amount) VALUES ($1, 'PO002', $2, 3, CURRENT_DATE, 'CNY', 10000)",
            &*po_id, &*supplier_id).execute(&mut *conn).await.unwrap();
        sqlx::query!("INSERT INTO purchase_order_lines (id, order_id, item_id, quantity, unit, unit_price, line_total, received_qty) VALUES ($1, $2, $3, 100, 'kg', 50, 5000, 0)",
            &*ID::new(), &*po_id, &*item).execute(&mut *conn).await.unwrap();

        let result = execute(&pool).await.unwrap();
        assert_eq!(result.len(), 0);
    }
}

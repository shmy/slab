// MRP 净需求计算。
//
// 计算逻辑委托给 PlanningPort::mrp_calculate。

use axum::extract::State;
use db::PgPool;
use planning_contract::port::MrpItem;
use web::response::json_response::{JsonResponse, JsonResponseType};

#[utoipa::path(
    get, path = "/api/v1/planning/mrp",
    operation_id = "planning_mrp", tag = "planning",
    responses((status = 200, body = JsonResponse<Vec<MrpItem>>)),
    security(("bearerAuth" = []))
)]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(State(pg_pool): State<PgPool>) -> JsonResponseType<Vec<MrpItem>> {
    let response = execute(&pg_pool).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip_all)]
#[inline]
async fn execute(pg_pool: &PgPool) -> rootcause::Result<Vec<MrpItem>> {
    let mut conn = pg_pool.acquire().await?;
    planning_contract::port::PlanningPort::mrp_calculate(&mut conn).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use appctx::testing;
    use migration::run_migrations;

    #[sqlx::test]
    async fn test_mrp(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let _state = testing::build(pool.clone()).await;
        let mut conn = pool.acquire().await.unwrap();

        let cat_id = shared_contract::value_object::id::ID::new();
        sqlx::query!(
            "INSERT INTO item_categories (id, name) VALUES ($1, 'Test')",
            &*cat_id
        )
        .execute(&mut *conn)
        .await
        .unwrap();

        let raw_item = shared_contract::value_object::id::ID::new();
        let fg_item = shared_contract::value_object::id::ID::new();
        sqlx::query!("INSERT INTO items (id, code, name, category_id, item_type, base_unit, safety_stock) VALUES ($1, 'RAW001', 'Plastic', $2, 1, 'kg', 500)",
            &*raw_item, &*cat_id).execute(&mut *conn).await.unwrap();
        sqlx::query!("INSERT INTO items (id, code, name, category_id, item_type, base_unit) VALUES ($1, 'FG001', 'ToyCar', $2, 5, 'pcs')",
            &*fg_item, &*cat_id).execute(&mut *conn).await.unwrap();

        // BOM: 1 ToyCar needs 2 kg Plastic
        let bom_id = shared_contract::value_object::id::ID::new();
        sqlx::query!("INSERT INTO boms (id, code, name, item_id, status) VALUES ($1, 'BOM001', 'ToyCarBOM', $2, 1)",
            &*bom_id, &*fg_item).execute(&mut *conn).await.unwrap();
        sqlx::query!("INSERT INTO bom_items (id, bom_id, item_id, quantity, unit) VALUES ($1, $2, $3, 2, 'kg')",
            &*shared_contract::value_object::id::ID::new(), &*bom_id, &*raw_item).execute(&mut *conn).await.unwrap();

        // Customer + Sales Order: 100 ToyCars = 200 kg Plastic
        let cust_id = shared_contract::value_object::id::ID::new();
        sqlx::query!("INSERT INTO customers (id, code, name, is_active) VALUES ($1, 'C001', 'TestCust', true)",
            &*cust_id).execute(&mut *conn).await.unwrap();
        let so_id = shared_contract::value_object::id::ID::new();
        sqlx::query!("INSERT INTO sales_orders (id, code, customer_id, status, order_date, currency, total_amount) VALUES ($1, 'SO001', $2, 0, CURRENT_DATE, 'CNY', 50000)",
            &*so_id, &*cust_id).execute(&mut *conn).await.unwrap();
        sqlx::query!("INSERT INTO sales_order_lines (id, order_id, item_id, quantity, unit, unit_price, line_total) VALUES ($1, $2, $3, 100, 'pcs', 500, 50000)",
            &*shared_contract::value_object::id::ID::new(), &*so_id, &*fg_item).execute(&mut *conn).await.unwrap();

        // Stock: 50 kg Plastic
        let wh_id = shared_contract::value_object::id::ID::new();
        sqlx::query!("INSERT INTO warehouses (id, code, name, type, is_active) VALUES ($1, 'WH01', 'Main', 1, true)",
            &*wh_id).execute(&mut *conn).await.unwrap();
        sqlx::query!("INSERT INTO inventories (id, item_id, warehouse_id, quantity, locked_qty, version) VALUES ($1, $2, $3, 50, 0, 1)",
            &*shared_contract::value_object::id::ID::new(), &*raw_item, &*wh_id).execute(&mut *conn).await.unwrap();

        let result = execute(&pool).await.unwrap();
        assert_eq!(result.len(), 1);
        let r = &result[0];
        assert_eq!(r.item_code, "RAW001");
        assert_eq!(r.gross_demand, 200);
        assert_eq!(r.current_stock, 50);
        assert_eq!(r.in_transit_qty, 0);
        assert_eq!(r.net_demand, 150);
        assert_eq!(r.safety_stock, 500);
        assert_eq!(r.suggested_order_qty, 650);
    }
    #[sqlx::test]
    async fn test_mrp_net_demand_floors_at_zero_when_stock_covers_demand(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let _state = testing::build(pool.clone()).await;
        let mut conn = pool.acquire().await.unwrap();

        let cat_id = shared_contract::value_object::id::ID::new();
        sqlx::query!(
            "INSERT INTO item_categories (id, name) VALUES ($1, 'Test')",
            &*cat_id
        )
        .execute(&mut *conn)
        .await
        .unwrap();
        let raw_item = shared_contract::value_object::id::ID::new();
        let fg_item = shared_contract::value_object::id::ID::new();
        sqlx::query!("INSERT INTO items (id, code, name, category_id, item_type, base_unit, safety_stock) VALUES ($1, 'RAW002', 'Plastic', $2, 1, 'kg', 500)",
            &*raw_item, &*cat_id).execute(&mut *conn).await.unwrap();
        sqlx::query!("INSERT INTO items (id, code, name, category_id, item_type, base_unit) VALUES ($1, 'FG002', 'ToyCar', $2, 5, 'pcs')",
            &*fg_item, &*cat_id).execute(&mut *conn).await.unwrap();
        let bom_id = shared_contract::value_object::id::ID::new();
        sqlx::query!("INSERT INTO boms (id, code, name, item_id, status) VALUES ($1, 'BOM002', 'BOM', $2, 1)",
            &*bom_id, &*fg_item).execute(&mut *conn).await.unwrap();
        sqlx::query!("INSERT INTO bom_items (id, bom_id, item_id, quantity, unit) VALUES ($1, $2, $3, 2, 'kg')",
            &*shared_contract::value_object::id::ID::new(), &*bom_id, &*raw_item).execute(&mut *conn).await.unwrap();
        let cust_id = shared_contract::value_object::id::ID::new();
        sqlx::query!(
            "INSERT INTO customers (id, code, name, is_active) VALUES ($1, 'C002', 'C', true)",
            &*cust_id
        )
        .execute(&mut *conn)
        .await
        .unwrap();
        let so_id = shared_contract::value_object::id::ID::new();
        sqlx::query!("INSERT INTO sales_orders (id, code, customer_id, status, order_date, currency, total_amount) VALUES ($1, 'SO002', $2, 0, CURRENT_DATE, 'CNY', 50000)",
            &*so_id, &*cust_id).execute(&mut *conn).await.unwrap();
        sqlx::query!("INSERT INTO sales_order_lines (id, order_id, item_id, quantity, unit, unit_price, line_total) VALUES ($1, $2, $3, 100, 'pcs', 500, 50000)",
            &*shared_contract::value_object::id::ID::new(), &*so_id, &*fg_item).execute(&mut *conn).await.unwrap();
        // 库存 250 > 毛需求 200 → 净需求下限 0
        let wh_id = shared_contract::value_object::id::ID::new();
        sqlx::query!("INSERT INTO warehouses (id, code, name, type, is_active) VALUES ($1, 'WH02', 'Main', 1, true)",
            &*wh_id).execute(&mut *conn).await.unwrap();
        sqlx::query!("INSERT INTO inventories (id, item_id, warehouse_id, quantity, locked_qty, version) VALUES ($1, $2, $3, 250, 0, 1)",
            &*shared_contract::value_object::id::ID::new(), &*raw_item, &*wh_id).execute(&mut *conn).await.unwrap();

        let result = execute(&pool).await.unwrap();
        let r = result.iter().find(|i| i.item_code == "RAW002").unwrap();
        assert_eq!(r.gross_demand, 200);
        assert_eq!(r.net_demand, 0); // GREATEST(..., 0) 下限
        assert_eq!(r.suggested_order_qty, 500); // 0 + safety_stock
    }

    #[sqlx::test]
    async fn test_mrp_demand_exactly_covered_by_stock(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let _state = testing::build(pool.clone()).await;
        let mut conn = pool.acquire().await.unwrap();

        let cat_id = shared_contract::value_object::id::ID::new();
        sqlx::query!(
            "INSERT INTO item_categories (id, name) VALUES ($1, 'Test')",
            &*cat_id
        )
        .execute(&mut *conn)
        .await
        .unwrap();
        let raw_item = shared_contract::value_object::id::ID::new();
        let fg_item = shared_contract::value_object::id::ID::new();
        sqlx::query!("INSERT INTO items (id, code, name, category_id, item_type, base_unit, safety_stock) VALUES ($1, 'RAW003', 'Plastic', $2, 1, 'kg', 500)",
            &*raw_item, &*cat_id).execute(&mut *conn).await.unwrap();
        sqlx::query!("INSERT INTO items (id, code, name, category_id, item_type, base_unit) VALUES ($1, 'FG003', 'ToyCar', $2, 5, 'pcs')",
            &*fg_item, &*cat_id).execute(&mut *conn).await.unwrap();
        let bom_id = shared_contract::value_object::id::ID::new();
        sqlx::query!("INSERT INTO boms (id, code, name, item_id, status) VALUES ($1, 'BOM003', 'BOM', $2, 1)",
            &*bom_id, &*fg_item).execute(&mut *conn).await.unwrap();
        sqlx::query!("INSERT INTO bom_items (id, bom_id, item_id, quantity, unit) VALUES ($1, $2, $3, 2, 'kg')",
            &*shared_contract::value_object::id::ID::new(), &*bom_id, &*raw_item).execute(&mut *conn).await.unwrap();
        let cust_id = shared_contract::value_object::id::ID::new();
        sqlx::query!(
            "INSERT INTO customers (id, code, name, is_active) VALUES ($1, 'C003', 'C', true)",
            &*cust_id
        )
        .execute(&mut *conn)
        .await
        .unwrap();
        let so_id = shared_contract::value_object::id::ID::new();
        sqlx::query!("INSERT INTO sales_orders (id, code, customer_id, status, order_date, currency, total_amount) VALUES ($1, 'SO003', $2, 0, CURRENT_DATE, 'CNY', 50000)",
            &*so_id, &*cust_id).execute(&mut *conn).await.unwrap();
        sqlx::query!("INSERT INTO sales_order_lines (id, order_id, item_id, quantity, unit, unit_price, line_total) VALUES ($1, $2, $3, 100, 'pcs', 500, 50000)",
            &*shared_contract::value_object::id::ID::new(), &*so_id, &*fg_item).execute(&mut *conn).await.unwrap();
        // 库存 200 恰好 = 毛需求 200 → 净需求 = 0（等号边界）
        let wh_id = shared_contract::value_object::id::ID::new();
        sqlx::query!("INSERT INTO warehouses (id, code, name, type, is_active) VALUES ($1, 'WH03', 'Main', 1, true)",
            &*wh_id).execute(&mut *conn).await.unwrap();
        sqlx::query!("INSERT INTO inventories (id, item_id, warehouse_id, quantity, locked_qty, version) VALUES ($1, $2, $3, 200, 0, 1)",
            &*shared_contract::value_object::id::ID::new(), &*raw_item, &*wh_id).execute(&mut *conn).await.unwrap();

        let result = execute(&pool).await.unwrap();
        let r = result.iter().find(|i| i.item_code == "RAW003").unwrap();
        assert_eq!(r.net_demand, 0);
        assert_eq!(r.suggested_order_qty, 500);
    }
}

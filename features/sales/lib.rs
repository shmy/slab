use feature::FeatureModule;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

mod endpoint;
mod repository;
mod shared;

pub struct Module;

impl FeatureModule for Module {
    fn name(&self) -> &'static str {
        "sales"
    }

    fn protected_routing(&self) -> OpenApiRouter<appctx::AppCtx> {
        OpenApiRouter::new()
            .routes(routes!(endpoint::sales_order_get::handler,))
            .routes(routes!(endpoint::sales_order_create::handler,))
            .routes(routes!(endpoint::sales_order_approve::submit_handler,))
            .routes(routes!(endpoint::sales_order_approve::approve_handler,))
            .routes(routes!(endpoint::sales_delivery_create::handler,))
            .routes(routes!(endpoint::sales_invoice_create::handler,))
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use db::PgPool;
    use http_auth::extract::operator::OperatorContext;
    use shared_contract::value_object::id::ID;
    use shared_contract::value_object::operator::Operator;

    /// 测试用操作人上下文（操作人 42，无 IP / UA）。
    pub fn test_operator_context() -> OperatorContext {
        OperatorContext(Operator {
            operator_id: ID::from(42),
            ip: None,
            user_agent: None,
        })
    }

    pub async fn insert_test_customer(pg_pool: &PgPool, code: &str) -> ID {
        let id = ID::new();
        let mut conn = pg_pool.acquire().await.unwrap();
        sqlx::query!(
            "INSERT INTO customers (id, code, name, is_active) VALUES ($1, $2, 'TestCustomer', true)",
            &*id,
            code,
        )
        .execute(&mut *conn)
        .await
        .unwrap();
        id
    }

    pub async fn insert_test_item(pg_pool: &PgPool, code: &str) -> ID {
        let id = ID::new();
        let mut conn = pg_pool.acquire().await.unwrap();
        sqlx::query!(
            "INSERT INTO items (id, code, name, item_type, base_unit) VALUES ($1, $2, 'TestItem', 1, 'kg')",
            &*id,
            code,
        )
        .execute(&mut *conn)
        .await
        .unwrap();
        id
    }

    pub async fn insert_test_warehouse(pg_pool: &PgPool, code: &str) -> ID {
        let id = ID::new();
        let mut conn = pg_pool.acquire().await.unwrap();
        sqlx::query!(
            "INSERT INTO warehouses (id, code, name, type, is_active) VALUES ($1, $2, 'TestWH', 3, true)",
            &*id,
            code,
        )
        .execute(&mut *conn)
        .await
        .unwrap();
        id
    }

    pub async fn insert_test_inventory(
        pg_pool: &PgPool,
        item_id: &ID,
        warehouse_id: &ID,
        quantity: i64,
    ) -> ID {
        let id = ID::new();
        let mut conn = pg_pool.acquire().await.unwrap();
        sqlx::query!(
            "INSERT INTO inventories (id, item_id, warehouse_id, quantity, locked_qty, version) VALUES ($1, $2, $3, $4, 0, 1)",
            &*id,
            item_id as _,
            warehouse_id as _,
            quantity,
        )
        .execute(&mut *conn)
        .await
        .unwrap();
        id
    }

    pub async fn insert_test_sales_order(
        pg_pool: &PgPool,
        code: &str,
        customer_id: &ID,
        status: i16,
    ) -> ID {
        let id = ID::new();
        let mut conn = pg_pool.acquire().await.unwrap();
        sqlx::query!(
            "INSERT INTO sales_orders (id, code, customer_id, status, order_date, currency, total_amount) VALUES ($1, $2, $3, $4, CURRENT_DATE, 'CNY', 10000)",
            &*id,
            code,
            customer_id as _,
            status,
        )
        .execute(&mut *conn)
        .await
        .unwrap();
        id
    }

    /// 插入一行销售订单明细（line_total = quantity * 100），返回 line id。
    pub async fn insert_test_sales_order_line(
        pg_pool: &PgPool,
        order_id: &ID,
        item_id: &ID,
        quantity: i64,
        delivered_qty: i64,
    ) -> ID {
        let id = ID::new();
        let mut conn = pg_pool.acquire().await.unwrap();
        let closed = delivered_qty >= quantity;
        sqlx::query!(
            "INSERT INTO sales_order_lines (id, order_id, line_no, item_id, quantity, unit, unit_price, line_total, delivered_qty, closed) VALUES ($1, $2, 1, $3, $4, 'kg', 100, $5, $6, $7)",
            &*id,
            order_id as _,
            item_id as _,
            quantity,
            quantity * 100,
            delivered_qty,
            closed,
        )
        .execute(&mut *conn)
        .await
        .unwrap();
        id
    }
}

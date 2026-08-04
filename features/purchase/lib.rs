use feature::FeatureModule;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

mod endpoint;
mod repository;
mod shared;

pub struct Module;

impl FeatureModule for Module {
    fn name(&self) -> &'static str {
        "purchase"
    }

    fn protected_routing(&self) -> OpenApiRouter<appctx::AppCtx> {
        OpenApiRouter::new()
            .routes(routes!(endpoint::purchase_order_get::handler,))
            .routes(routes!(endpoint::purchase_order_search::handler,))
            .routes(routes!(endpoint::purchase_receipt_get::handler,))
            .routes(routes!(endpoint::purchase_return_get::handler,))
            .routes(routes!(endpoint::purchase_invoice_get::handler,))
            .routes(routes!(
                endpoint::purchase_order_create::handler,
                endpoint::purchase_order_delete::handler,
            ))
            .routes(routes!(endpoint::purchase_order_submit::handler,))
            .routes(routes!(endpoint::purchase_order_approve::handler,))
            .routes(routes!(endpoint::purchase_order_reject::handler,))
            .routes(routes!(endpoint::purchase_receipt_create::handler,))
            .routes(routes!(endpoint::purchase_invoice_create::handler,))
            .routes(routes!(endpoint::purchase_return_create::handler,))
            .routes(routes!(endpoint::purchase_return_approve::submit_handler,))
            .routes(routes!(endpoint::purchase_return_approve::approve_handler,))
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use db::PgPool;
    use shared_contract::value_object::id::ID;

    /// 插入一个供应商 + 指定状态的采购订单，返回订单 ID。
    pub async fn insert_test_purchase_order(pg_pool: &PgPool, code: &str, status: i16) -> ID {
        let supplier_id = ID::new();
        let order_id = ID::new();
        let mut conn = pg_pool.acquire().await.unwrap();
        sqlx::query!(
            "INSERT INTO suppliers (id, code, name, is_active) VALUES ($1, $2, 'TestSupplier', true)",
            &*supplier_id,
            code,
        )
        .execute(&mut *conn)
        .await
        .unwrap();
        sqlx::query!(
            r#"INSERT INTO purchase_orders (id, code, supplier_id, status, order_date, currency, total_amount)
               VALUES ($1, $2, $3, $4, CURRENT_DATE, 'CNY', 10000)"#,
            &*order_id,
            code,
            &*supplier_id,
            status,
        )
        .execute(&mut *conn)
        .await
        .unwrap();
        order_id
    }
}

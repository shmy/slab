use feature::FeatureModule;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

mod endpoint;
mod repository;
mod shared;

pub struct Module;

impl FeatureModule for Module {
    fn name(&self) -> &'static str {
        "warehouse"
    }

    fn protected_routing(&self) -> OpenApiRouter<appctx::AppCtx> {
        OpenApiRouter::new()
            .routes(routes!(
                endpoint::warehouse_create::handler,
                endpoint::warehouse_list::handler,
            ))
            .routes(routes!(
                endpoint::inventory_search::handler,
                endpoint::inventory_initial::handler,
            ))
            .routes(routes!(
                endpoint::warehouse_delete::handler,
                endpoint::warehouse_update::handler,
            ))
            .routes(routes!(endpoint::inventory_transaction_search::handler,))
            .routes(routes!(endpoint::inventory_check_create::handler,))
            .routes(routes!(endpoint::inventory_check_submit::handler,))
            .routes(routes!(endpoint::inventory_check_approve::handler,))
            .routes(routes!(endpoint::stock_transfer_create::handler,))
            .routes(routes!(endpoint::stock_transfer_submit::handler,))
            .routes(routes!(endpoint::stock_transfer_approve::handler,))
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use db::PgPool;
    use shared_contract::value_object::id::ID;

    /// 插入一个测试仓库，返回仓库 ID。
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
}

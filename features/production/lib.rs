use feature::FeatureModule;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

mod endpoint;

pub struct Module;

impl FeatureModule for Module {
    fn name(&self) -> &'static str {
        "production"
    }

    fn protected_routing(&self) -> OpenApiRouter<appctx::AppCtx> {
        OpenApiRouter::new()
            .routes(routes!(endpoint::work_order_get::handler,))
            .routes(routes!(endpoint::work_order_create::handler,))
            .routes(routes!(endpoint::work_order_release::handler,))
            .routes(routes!(endpoint::work_order_material_pick::handler,))
            .routes(routes!(endpoint::work_order_operation_report::handler,))
            .routes(routes!(endpoint::work_order_complete::handler,))
            .routes(routes!(endpoint::production_receipt_create::handler,))
            .routes(routes!(endpoint::work_order_material_cost_get::handler,))
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use db::PgPool;
    use shared_contract::value_object::id::ID;

    /// 插入一个测试物料，返回物料 ID。
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

    /// 插入一个测试 BOM（status=1 已发布），返回 BOM ID。
    pub async fn insert_test_bom(pg_pool: &PgPool, code: &str, item_id: &ID) -> ID {
        let id = ID::new();
        let mut conn = pg_pool.acquire().await.unwrap();
        sqlx::query!(
            "INSERT INTO boms (id, code, name, item_id, status) VALUES ($1, $2, 'TestBOM', $3, 1)",
            &*id,
            code,
            item_id as _,
        )
        .execute(&mut *conn)
        .await
        .unwrap();
        id
    }
}

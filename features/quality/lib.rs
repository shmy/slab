use feature::FeatureModule;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

mod endpoint;

pub struct Module;

impl FeatureModule for Module {
    fn name(&self) -> &'static str {
        "quality"
    }

    fn protected_routing(&self) -> OpenApiRouter<appctx::AppCtx> {
        OpenApiRouter::new()
            .routes(routes!(endpoint::inspection_template_get::handler,))
            .routes(routes!(endpoint::inspection_order_get::handler,))
            .routes(routes!(endpoint::non_conformance_get::handler,))
            .routes(routes!(endpoint::inspection_template_create::handler,))
            .routes(routes!(endpoint::inspection_order_create::handler,))
            .routes(routes!(endpoint::inspection_order_complete::handler,))
            .routes(routes!(endpoint::non_conformance_create::handler,))
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

    /// 插入检验模板（category=1 IQC），返回 id。
    pub async fn insert_test_template(pg_pool: &PgPool, code: &str) -> ID {
        let id = ID::new();
        let mut conn = pg_pool.acquire().await.unwrap();
        sqlx::query!(
            "INSERT INTO inspection_templates (id, code, name, category) VALUES ($1, $2, 'Template', 1)",
            &*id,
            code,
        )
        .execute(&mut *conn)
        .await
        .unwrap();
        id
    }

    /// 插入模板明细项，返回 id。
    pub async fn insert_test_template_item(pg_pool: &PgPool, template_id: &ID) -> ID {
        let id = ID::new();
        let mut conn = pg_pool.acquire().await.unwrap();
        sqlx::query!(
            "INSERT INTO inspection_template_items (id, template_id, name, is_required, sort_order) VALUES ($1, $2, 'CheckItem', true, 0)",
            &*id,
            template_id as _,
        )
        .execute(&mut *conn)
        .await
        .unwrap();
        id
    }

    /// 插入检验单（status=0），返回 id。
    pub async fn insert_test_inspection_order(
        pg_pool: &PgPool,
        code: &str,
        template_id: &ID,
        item_id: &ID,
    ) -> ID {
        let id = ID::new();
        let mut conn = pg_pool.acquire().await.unwrap();
        sqlx::query!(
            "INSERT INTO inspection_orders (id, code, template_id, source_type, source_id, item_id, lot_qty, sample_qty, status) VALUES ($1, $2, $3, 'purchase_receipt', 0, $4, 100, 10, 0)",
            &*id,
            code,
            template_id as _,
            item_id as _,
        )
        .execute(&mut *conn)
        .await
        .unwrap();
        id
    }
}

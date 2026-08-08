use module::DomainModule;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

mod endpoint;

pub struct Module;

impl DomainModule for Module {
    fn name(&self) -> &'static str {
        "product"
    }

    fn protected_routing(&self) -> OpenApiRouter<appctx::AppCtx> {
        OpenApiRouter::new()
            .routes(routes!(endpoint::bom_get::handler,))
            .routes(routes!(endpoint::mold_get::handler,))
            .routes(routes!(endpoint::bom_create::handler,))
            .routes(routes!(endpoint::bom_release::handler,))
            .routes(routes!(endpoint::mold_create::handler,))
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

    /// 插入一个测试 BOM（status=0 草稿），返回 BOM ID。
    pub async fn insert_test_bom(pg_pool: &PgPool, code: &str, item_id: &ID) -> ID {
        let id = ID::new();
        let mut conn = pg_pool.acquire().await.unwrap();
        sqlx::query!(
            "INSERT INTO boms (id, code, name, item_id, status) VALUES ($1, $2, 'TestBOM', $3, 0)",
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

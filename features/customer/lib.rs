use module::DomainModule;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

mod endpoint;
mod repository;

/// 筛选白名单（协议事实源，bin/server meta 端点收集）。
pub use endpoint::customer_search::FILTER_SCHEMA;

pub struct Module;

impl DomainModule for Module {
    fn name(&self) -> &'static str {
        "customer"
    }

    fn protected_routing(&self) -> OpenApiRouter<appctx::AppCtx> {
        OpenApiRouter::new()
            .routes(routes!(
                endpoint::customer_create::handler,
                endpoint::customer_search::handler,
            ))
            .routes(routes!(
                endpoint::customer_get::handler,
                endpoint::customer_update::handler,
                endpoint::customer_delete::handler,
            ))
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

    /// 直接插入一条客户用于写端点测试。
    pub async fn insert_test_customer(pg_pool: &PgPool, code: &str, name: &str) -> ID {
        let id = ID::new();
        let mut conn = pg_pool.acquire().await.unwrap();
        sqlx::query!(
            r#"INSERT INTO customers (id, code, name, contact_person, phone, address, payment_terms, is_active)
               VALUES ($1, $2, $3, NULL, NULL, NULL, NULL, TRUE)"#,
            &*id,
            code,
            name,
        )
        .execute(&mut *conn)
        .await
        .unwrap();
        id
    }
}

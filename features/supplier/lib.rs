use module::DomainModule;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

mod endpoint;
mod repository;

#[cfg(test)]
pub(crate) mod tests {
    use db::PgPool;
    use http_auth::extract::operator::OperatorContext;
    use shared_contract::value_object::id::ID;
    use shared_contract::value_object::operator::Operator;
    use supplier_contract::entity::Supplier;

    /// 测试用操作人上下文（操作人 42，无 IP / UA）。
    pub fn test_operator_context() -> OperatorContext {
        OperatorContext(Operator {
            operator_id: ID::from(42),
            ip: None,
            user_agent: None,
        })
    }

    /// 插入一条测试供应商，返回其 ID。
    pub async fn insert_test_supplier(pg_pool: &PgPool) -> ID {
        let id = ID::new();
        let supplier = Supplier {
            id,
            code: format!("S-{}", i64::from(id)),
            name: "Test Supplier".to_string(),
            contact_person: Some("Contact".to_string()),
            phone: None,
            address: None,
            payment_terms: None,
            is_active: true,
        };
        let mut conn = pg_pool.acquire().await.unwrap();
        crate::repository::supplier_repository::SupplierRepository::create(&mut conn, &supplier)
            .await
            .unwrap();
        id
    }
}

pub struct Module;

impl DomainModule for Module {
    fn name(&self) -> &'static str {
        "supplier"
    }

    fn protected_routing(&self) -> OpenApiRouter<appctx::AppCtx> {
        OpenApiRouter::new()
            .routes(routes!(
                endpoint::supplier_create::handler,
                endpoint::supplier_search::handler,
            ))
            .routes(routes!(
                endpoint::supplier_get::handler,
                endpoint::supplier_update::handler,
                endpoint::supplier_delete::handler,
            ))
    }
}

use module::DomainModule;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

mod endpoint;

pub struct Module;

impl DomainModule for Module {
    fn name(&self) -> &'static str {
        "finance"
    }

    fn protected_routing(&self) -> OpenApiRouter<appctx::AppCtx> {
        OpenApiRouter::new()
            .routes(routes!(endpoint::payment_create::handler,))
            .routes(routes!(endpoint::payment_get::handler,))
            .routes(routes!(endpoint::payment_search::handler,))
            .routes(routes!(endpoint::aging::handler,))
            .routes(routes!(endpoint::income_statement::handler,))
            .routes(routes!(endpoint::balances::handler,))
    }
}

#[cfg(test)]
pub(crate) mod tests {
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
}

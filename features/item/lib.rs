use feature::FeatureModule;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

mod endpoint;
mod repository;

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

pub struct Module;

impl FeatureModule for Module {
    fn name(&self) -> &'static str {
        "item"
    }

    fn protected_routing(&self) -> OpenApiRouter<appctx::AppCtx> {
        OpenApiRouter::new()
            .routes(routes!(
                endpoint::item_create::handler,
                endpoint::item_search::handler,
            ))
            .routes(routes!(
                endpoint::item_get::handler,
                endpoint::item_update::handler,
                endpoint::item_delete::handler,
            ))
            .routes(routes!(
                endpoint::item_category_create::handler,
                endpoint::item_category_update::handler,
                endpoint::item_category_delete::handler,
                endpoint::item_category_tree::handler,
            ))
            .routes(routes!(
                endpoint::item_unit_create::handler,
                endpoint::item_unit_list::handler,
            ))
            .routes(routes!(
                endpoint::item_cost_create::handler,
                endpoint::item_cost_list::handler,
            ))
            .routes(routes!(endpoint::item_weighted_cost_get::handler,))
    }
}

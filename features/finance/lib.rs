use feature::FeatureModule;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

mod endpoint;

pub struct Module;

impl FeatureModule for Module {
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

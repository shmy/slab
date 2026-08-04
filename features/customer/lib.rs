use feature::FeatureModule;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

mod endpoint;
mod repository;

pub struct Module;

impl FeatureModule for Module {
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

use feature::FeatureModule;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

mod endpoint;
mod repository;

pub struct Module;

impl FeatureModule for Module {
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

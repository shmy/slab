use appctx::AppCtx;
use feature::FeatureModule;
use utoipa_axum::{router::OpenApiRouter, routes};

mod endpoint;

pub struct Module;

impl FeatureModule for Module {
    fn name(&self) -> &'static str {
        "health"
    }

    fn unprotected_routing(&self) -> OpenApiRouter<AppCtx> {
        OpenApiRouter::new()
            .routes(routes!(endpoint::livez::handler))
            .routes(routes!(endpoint::readyz::handler))
            .routes(routes!(endpoint::healthz::handler))
    }
}

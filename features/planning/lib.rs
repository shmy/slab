use module::DomainModule;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

mod endpoint;

pub struct Module;

impl DomainModule for Module {
    fn name(&self) -> &'static str {
        "planning"
    }

    fn protected_routing(&self) -> OpenApiRouter<appctx::AppCtx> {
        OpenApiRouter::new()
            .routes(routes!(endpoint::reorder_alert_list::handler,))
            .routes(routes!(endpoint::purchase_suggestion_list::handler,))
            .routes(routes!(endpoint::mrp_calculate::handler,))
    }
}

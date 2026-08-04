use feature::FeatureModule;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

mod endpoint;
mod repository;

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

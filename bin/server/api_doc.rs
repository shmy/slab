use utoipa::openapi::security::SecurityScheme::Http as SecuritySchemeHttp;
use utoipa::{
    Modify,
    openapi::{
        OpenApi,
        security::{Http, HttpAuthScheme::Bearer},
    },
};

// 根 OpenAPI：全局 `info`、安全方案；具体 path 由各域 `OpenApiRouter::routes(utoipa_axum::routes!(...))` 从带 `#[utoipa::path]` 的 handler 收集。
#[derive(utoipa::OpenApi)]
#[openapi(
    modifiers(&SecurityAddon),
    info(
        title = "Slab API", 
        description = concat!(
            include_str!("api_doc/introduction.md")
        )
    ),
)]
pub(crate) struct ApiDoc;

struct SecurityAddon;

impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut OpenApi) {
        if let Some(components) = openapi.components.as_mut() {
            components.add_security_scheme("bearerAuth", SecuritySchemeHttp(Http::new(Bearer)));
        }
    }
}

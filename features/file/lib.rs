use appctx::AppCtx;
use axum::extract::DefaultBodyLimit;
use module::DomainModule;
use utoipa_axum::{
    router::{OpenApiRouter, UtoipaMethodRouterExt},
    routes,
};

mod endpoint;

/// 单文件最大 2MiB，整体 body 略放大以容纳 boundary 等开销。
const FILE_UPLOAD_BODY_LIMIT: usize = 3 * 1024 * 1024;

pub struct Module;

impl DomainModule for Module {
    fn name(&self) -> &'static str {
        "file"
    }

    fn protected_routing(&self) -> OpenApiRouter<AppCtx> {
        OpenApiRouter::new().routes(
            routes!(endpoint::file_upload_image::handler)
                .layer(DefaultBodyLimit::max(FILE_UPLOAD_BODY_LIMIT)),
        )
    }
}

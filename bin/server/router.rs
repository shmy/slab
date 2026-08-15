use std::time::Duration;

use appctx::AppCtx;
use axum::{Router, extract::DefaultBodyLimit, http::StatusCode, middleware};
use axum_governor::{GovernorConfigBuilder, GovernorLayer, Quota, SmartIp, nz};
use axum_tracing_opentelemetry::middleware::{OtelAxumLayer, OtelInResponseLayer};
use http_auth::middleware::account_auth_middleware;
use locale::middleware::l10n_middleware;
use tower::ServiceBuilder;
use tower_http::timeout::TimeoutLayer;
use utoipa::OpenApi as _;
use utoipa_axum::router::OpenApiRouter;
use utoipa_scalar::Servable as _;

use crate::metrics::record_request_metrics;
use crate::{api_doc::ApiDoc, modules::MODULES};

const CUSTOM_HTML: &str = include_str!("scalar.html");

pub fn build(state: AppCtx, request_timeout: Duration, scalar_ui_enabled: bool) -> Router {
    let cfg = GovernorConfigBuilder::default()
        .with_extractor(SmartIp::default())
        .expect_connect_info()
        .quota_default(Quota::requests_per_second(nz!(50u32)))
        .finish()
        .expect("build governor config");

    let mut protected = OpenApiRouter::new();
    let mut unprotected = OpenApiRouter::new();
    for module in MODULES {
        protected = protected.merge(module.protected_routing());
        unprotected = unprotected.merge(module.unprotected_routing());
    }

    let (api_router, open_api) = OpenApiRouter::with_openapi(ApiDoc::openapi())
        .layer(DefaultBodyLimit::max(2 * 1024 * 1024))
        .merge(protected)
        .layer(middleware::from_fn_with_state(
            state.clone(),
            account_auth_middleware,
        ))
        .merge(unprotected)
        .layer(middleware::from_fn(l10n_middleware))
        .layer(ServiceBuilder::new().layer(GovernorLayer::new(cfg)))
        .layer(OtelInResponseLayer)
        .layer(OtelAxumLayer::default())
        .layer(middleware::from_fn(record_request_metrics))
        .with_state(state)
        .split_for_parts();

    let mut router = Router::new().merge(api_router);

    // 机器可读契约端点：OpenAPI JSON（前端 pnpm gen:api 直接拉取）
    router = router.route(
        "/openapi.json",
        axum::routing::get({
            let spec = open_api.clone();
            move || {
                let spec = spec.clone();
                async move { axum::Json(spec) }
            }
        }),
    );

    // 筛选协议元数据（前端 gen:api 拉取 → 生成 src/lib/filter-schema.ts）
    router = router.route(
        "/api/v1/meta/filter-schemas",
        axum::routing::get(|| async { crate::meta::handler() }),
    );

    if scalar_ui_enabled {
        router = router
            .merge(utoipa_scalar::Scalar::with_url("/scalar", open_api).custom_html(CUSTOM_HTML));
    }

    // 最外层：整请求从进入到响应 future 完成的上限（含 Scalar、/healthz、/livez、/readyz 等全部挂载点）。
    router.layer(TimeoutLayer::with_status_code(
        StatusCode::REQUEST_TIMEOUT,
        request_timeout,
    ))
}

use std::sync::LazyLock;
use std::time::Instant;

use axum::{Json, extract::State};
use db::PgPool;
use serde::Serialize;
use utoipa::ToSchema;

static PROCESS_START: LazyLock<Instant> = LazyLock::new(Instant::now);

#[derive(Serialize, ToSchema)]
pub(crate) struct HealthzProcess {
    pub uptime_seconds: u64,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct HealthzBuild {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_tag: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct HealthzPostgres {
    pub size: u32,
    pub num_idle: u32,
    pub max_connections: u32,
    pub is_closed: bool,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct HealthzResponse {
    pub process: HealthzProcess,
    pub build: HealthzBuild,
    pub postgres: HealthzPostgres,
}

#[utoipa::path(
    get,
    path = "/healthz",
    operation_id = "healthz",
    tag = "health",
    responses((status = 200, body = HealthzResponse)),
)]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(State(pg_pool): State<PgPool>) -> Json<HealthzResponse> {
    let options = pg_pool.options();

    Json(HealthzResponse {
        process: HealthzProcess {
            uptime_seconds: PROCESS_START.elapsed().as_secs(),
        },
        build: HealthzBuild {
            image_tag: std::env::var("IMAGE_TAG").ok(),
        },
        postgres: HealthzPostgres {
            size: options.get_max_connections(),
            num_idle: 0,
            max_connections: options.get_min_connections(),
            is_closed: false,
        },
    })
}

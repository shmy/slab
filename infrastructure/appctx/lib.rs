#[cfg(feature = "test-utils")]
pub mod testing;

use std::fmt::Debug;

use axum::extract::FromRef;
pub use blob::Blob;
pub use cache::KvBackend;
pub use db::PgPool;
pub use flow::Flow;
pub use http_client::HttpClient;
pub use jwt::{TokenBundle, TokenHelper, TokenRealm};
pub use queue::QueueBackend;
use tracing::info;

#[derive(Clone, FromRef)]
pub struct AppCtx {
    pub pg_pool: PgPool,
    /// 缓存后端（`cache::Backend`，变体由组装处选择：Pg / Redb / Redis）。
    pub kv: KvBackend,
    /// 队列后端（`queue::Backend`，变体由组装处选择：Pg / Nats）。
    pub queue: QueueBackend,
    pub token_bundle: TokenBundle,
    pub http_client: HttpClient,
    pub blob: Blob,
    pub flow: Flow,
}

impl Debug for AppCtx {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppCtx").finish()
    }
}

impl AppCtx {
    pub async fn clear(&self) {
        info!("Disconnecting postgresql...");
        self.pg_pool.close().await;
    }
}

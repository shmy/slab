use appctx::{AppCtx, Flow, HttpClient, TokenBundle};
use appctx::{TokenHelper, TokenRealm};
use blob::{Blob, BlobConfig};
use cache::Backend;
use db::{DbConfig, PgPool, connect};
use rootcause::Result;
use secrecy::ExposeSecret;

use crate::cli::Cli;

pub async fn build_app_ctx(cli: &Cli) -> Result<AppCtx> {
    let flow = Flow::try_new(cli.database.url.expose_secret()).await?;
    let (pg_pool, blob) = tokio::try_join!(connect_postgresql(cli), connect_s3(cli),)?;

    // 缓存后端（互斥，开启其一；default=kv-pg 与显式 kv-redb/kv-redis 并集时 pg 分支让位）：
    #[cfg(feature = "kv-pg")]
    let kv = Backend::try_new(pg_pool.clone()).await?;
    #[cfg(feature = "kv-redb")]
    let kv = Backend::try_new(&cli.cache.db_path)?;
    #[cfg(feature = "kv-redis")]
    let kv = Backend::try_new(&cli.cache.url).await?;

    Ok(AppCtx {
        pg_pool,
        kv,
        token_bundle: TokenBundle::new(
            TokenHelper::new(
                TokenRealm::Customer,
                cli.jwt.secret.clone(),
                cli.jwt.access_ttl_secs(),
                cli.jwt.refresh_ttl_secs(),
            ),
            TokenHelper::new(
                TokenRealm::Account,
                cli.jwt.secret.clone(),
                cli.jwt.access_ttl_secs(),
                cli.jwt.refresh_ttl_secs(),
            ),
        ),
        http_client: HttpClient::default(),
        blob,
        flow,
    })
}

async fn connect_postgresql(cli: &Cli) -> Result<PgPool> {
    connect(DbConfig {
        url: cli.database.url.clone(),
        max_connections: cli.database.max_connections,
        min_connections: cli.database.min_connections,
        max_lifetime: cli.database.max_lifetime.duration(),
        idle_timeout: cli.database.idle_timeout.duration(),
        acquire_timeout: cli.database.acquire_timeout.duration(),
    })
    .await
}

async fn connect_s3(cli: &Cli) -> Result<Blob> {
    Blob::try_new(BlobConfig {
        endpoint: &cli.s3.endpoint,
        domain: &cli.s3.domain,
        bucket: &cli.s3.bucket,
        secret_id: cli.s3.secret_id.expose_secret(),
        secret_key: cli.s3.secret_key.expose_secret(),
    })
    .await
}

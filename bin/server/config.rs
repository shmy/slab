use appctx::{AppCtx, Flow, HttpClient, TokenBundle};
use appctx::{TokenHelper, TokenRealm};
use blob::Blob;
#[cfg(all(feature = "blob-cos", not(feature = "blob-fs")))]
use blob::CosConfig;
#[cfg(feature = "blob-fs")]
use blob::FsConfig;
use kv::KvBackend;
use db::{DbConfig, PgPool, connect};
use queue::QueueBackend;
use rootcause::Result;
use secrecy::ExposeSecret;

use crate::cli::Cli;

pub async fn build_app_ctx(cli: &Cli) -> Result<AppCtx> {
    let flow = Flow::try_new(cli.database.url.expose_secret()).await?;
    let (pg_pool, blob) = tokio::try_join!(connect_postgresql(cli), connect_blob(cli),)?;

    // 缓存后端（互斥，开启其一；default=kv-redis，并集时 pg 分支让位）：
    #[cfg(all(feature = "kv-pg", not(any(feature = "kv-redb", feature = "kv-redis"))))]
    let kv = KvBackend::try_new_pg(pg_pool.clone()).await?;
    #[cfg(feature = "kv-redb")]
    let kv = KvBackend::try_new_redb(&cli.cache.db_path)?;
    #[cfg(feature = "kv-redis")]
    let kv = {
        use kv::{Pool, RedisConnectionManager};
        let manager = RedisConnectionManager::new(cli.cache.url.clone())?;
        let pool = Pool::builder().max_size(16).build(manager).await?;
        KvBackend::try_new_redis(pool).await?
    };

    // 队列后端：默认（或 queue-pg）→ Outbox + 进程内 dispatcher；queue-nats → JetStream 直发。
    #[cfg(all(feature = "queue-pg", not(feature = "queue-nats")))]
    let queue = QueueBackend::try_new_pg(pg_pool.clone()).await?;
    #[cfg(feature = "queue-nats")]
    let queue = QueueBackend::try_new_nats(queue::NatsConfig {
        url: cli.nats.url.clone(),
        username: cli.nats.username.clone(),
        password: cli.nats.password.clone(),
        stream_name: cli.nats.stream_name.clone(),
    })
    .await?;

    Ok(AppCtx {
        pg_pool,
        kv,
        queue,
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

async fn connect_blob(cli: &Cli) -> Result<Blob> {
    // blob 后端：默认 blob-cos（腾讯云 COS / S3 兼容）；blob-fs → 本地文件系统。
    #[cfg(all(feature = "blob-cos", not(feature = "blob-fs")))]
    {
        Blob::try_new_cos(CosConfig {
            endpoint: &cli.s3.endpoint,
            domain: &cli.s3.domain,
            bucket: &cli.s3.bucket,
            secret_id: cli.s3.secret_id.expose_secret(),
            secret_key: cli.s3.secret_key.expose_secret(),
        })
        .await
    }
    #[cfg(feature = "blob-fs")]
    {
        Blob::try_new_fs(FsConfig {
            root: &cli.fs.root,
            domain: &cli.fs.domain,
        })
        .await
    }
}

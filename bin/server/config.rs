use appctx::{AppCtx, Flow, HttpClient, TokenBundle};
use appctx::{TokenHelper, TokenRealm};
use blob::Blob;
#[cfg(all(feature = "blob-cos", not(feature = "blob-fs")))]
use blob::CosConfig;
#[cfg(feature = "blob-fs")]
use blob::FsConfig;
use db::{DbConfig, PgPool, connect};
use event_bus::EventBus;
use kv::KvBackend;
use rootcause::Result;
use secrecy::ExposeSecret;
use worker::JobBus;

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

    // 事件总线后端：默认（或 event-bus-pg）→ Outbox + 进程内 dispatcher；event-bus-nats → JetStream 直发。
    #[cfg(all(feature = "event-bus-pg", not(feature = "event-bus-nats")))]
    let bus = EventBus::try_new_pg(pg_pool.clone()).await?;
    #[cfg(feature = "event-bus-nats")]
    let bus = EventBus::try_new_nats(event_bus::NatsConfig {
        url: cli.nats.url.clone(),
        username: cli.nats.username.clone(),
        password: cli.nats.password.clone(),
        stream_name: cli.nats.stream_name.clone(),
    })
    .await?;

    let jobs = build_job_bus(cli, pg_pool.clone()).await?;

    Ok(AppCtx {
        pg_pool,
        kv,
        bus,
        jobs,
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

async fn build_job_bus(_cli: &Cli, _pg_pool: PgPool) -> Result<JobBus> {
    // 队列后端（互斥，开启其一；双开时非默认的 worker-pg 让 sqlite 让位，同 blob 惯例）：
    //   worker-pg      → worker_jobs 表在业务 PG（生产；显式指定时优先）；
    //   worker-sqlite  → worker_jobs 表在本地 sqlite 文件（单机部署，默认）。
    // 语义见 docs/JOB_QUEUE.md。
    #[cfg(feature = "worker-pg")]
    {
        JobBus::try_new_pg(_pg_pool).await
    }
    #[cfg(all(feature = "worker-sqlite", not(feature = "worker-pg")))]
    {
        use worker::sqlite_helper::new_sqlite_pool;
        let pool = new_sqlite_pool(&_cli.queue.sqlite_path).await?;
        JobBus::try_new_sqlite(pool).await
    }
}

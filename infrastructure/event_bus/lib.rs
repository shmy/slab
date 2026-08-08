//! 事件总线：`EventBus` 枚举 + 方法门面（模式与 `infrastructure/kv` 一致）。
//!
//! - `PgBackend`：feature `pg`（**默认**），Outbox 表 + 进程内 dispatcher，发布与业务同事务。
//! - `NatsBackend`：feature `nats`，JetStream 直发（延迟用 ADR-51 schedule），发布不参与 PG 事务。
//!
//! **发布语义差异**：pg 与业务同事务（回滚即不投递）；nats 直发（业务回滚事件仍已投递）——
//! 消费端 subscriber 必须幂等。**消费端幂等**均靠 subscriber 实现（at-least-once）。

mod dispatcher;
pub mod event;
mod gc;
#[cfg(feature = "nats")]
mod nats;
mod pg;
pub mod publish;
mod registry;
mod status;
mod subscriber;

use std::time::Duration;

#[cfg(feature = "pg")]
use db::PgPool;
use rootcause::Result;
#[cfg(feature = "pg")]
use sqlx::PgConnection;
use tokio::sync::watch::Receiver;

pub use event::Event;
#[cfg(feature = "nats")]
pub use nats::{NatsBackend, NatsConfig};
pub use pg::PgBackend;
pub use registry::{FrozenRegistry, Registry};
pub use subscriber::Subscriber;

pub use gc::{DEFAULT_DELIVERED_RETENTION_DAYS, delete_delivered_older_than_in_transaction};

#[cfg(not(any(feature = "pg", feature = "nats")))]
compile_error!("event_bus crate requires feature \"pg\" or \"nats\"");

/// 事件总线句柄：发布 + 消费 + 清理，克隆共享。
#[derive(Clone)]
pub enum EventBus {
    #[cfg(feature = "pg")]
    Pg(PgBackend),
    #[cfg(feature = "nats")]
    Nats(Box<NatsBackend>),
}

impl EventBus {
    /// 各后端构造器独立命名：同名 `try_new` 在 feature 并集下会因方法重名冲突（Rust 无重载），
    /// 拆名后 pg 与 nats 可并存（与 kv 的 `try_new_pg/redb/redis` 同构）。
    #[cfg(feature = "pg")]
    pub async fn try_new_pg(pg_pool: PgPool) -> Result<Self> {
        Ok(Self::Pg(PgBackend::try_new(pg_pool).await?))
    }

    /// 测试用后端：复用测试 PG 池（`PgBackend::try_new` 幂等建表，不依赖 NATS 实例）。
    #[cfg(feature = "test-utils")]
    pub async fn new_for_test(pg_pool: db::PgPool) -> Result<Self> {
        Ok(Self::Pg(PgBackend::try_new(pg_pool).await?))
    }

    /// NATS 后端：`config` 为 JetStream 连接参数；消费上下文（如 AppCtx）由 `run_dispatcher` 传入。
    #[cfg(feature = "nats")]
    pub async fn try_new_nats(config: NatsConfig) -> Result<Self> {
        let nats = NatsBackend::try_new(config).await?;
        Ok(Self::Nats(Box::new(nats)))
    }

    /// 发布。pg：写 outbox 表（与业务同事务）；nats：JetStream 直发（忽略 `executor`）。
    pub async fn publish<T: Event>(&self, event: &T) -> Result<()> {
        match self {
            #[cfg(feature = "pg")]
            Self::Pg(b) => b.publish(event).await,
            #[cfg(feature = "nats")]
            Self::Nats(b) => b.publish(event).await,
        }
    }

    /// 事务内发布（**强一致，pg 后端**）：与业务同一事务，回滚即不投递（Outbox 语义）。
    /// nats 后端无事务语义，等价于 `publish`（直发 JetStream，忽略 `executor`）。
    /// 需要「业务提交成功则消息必落库」的可靠性时用本方法；普通通知类事件用 `publish`。
    pub async fn publish_in_tx<T: Event>(
        &self,
        executor: &mut PgConnection,
        event: &T,
    ) -> Result<()> {
        match self {
            #[cfg(feature = "pg")]
            Self::Pg(b) => b.publish_in_tx(executor, event).await,
            #[cfg(feature = "nats")]
            Self::Nats(b) => b.publish(event).await,
        }
    }

    /// 延迟发布。pg：`next_attempt_at` 门控；nats：JetStream ADR-51 `@at` 调度。
    pub async fn publish_with_delay<T: Event>(&self, event: &T, delay: Duration) -> Result<()> {
        match self {
            #[cfg(feature = "pg")]
            Self::Pg(b) => b.publish_delayed(event, delay).await,
            #[cfg(feature = "nats")]
            Self::Nats(b) => b.publish_delayed(event, delay).await,
        }
    }

    /// 启动消费循环（阻塞直到 shutdown）。pg：进程内轮询 outbox；nats：每 subscriber 一个 durable consumer。
    /// `ctx` 为消费上下文（如 `AppCtx`），原样传给每个 `Subscriber::handle`。
    pub async fn run_dispatcher<C: Send + Sync + Clone + 'static>(
        &self,
        ctx: C,
        registry: FrozenRegistry<C>,
        shutdown: Receiver<bool>,
    ) -> Result<()> {
        match self {
            #[cfg(feature = "pg")]
            Self::Pg(b) => b.run_dispatcher(ctx, registry, shutdown).await,
            #[cfg(feature = "nats")]
            Self::Nats(b) => {
                nats::run_nats_dispatcher(b, ctx, registry, shutdown, nats::DEFAULT_ACK_WAIT).await
            }
        }
    }

    /// 清理已投递旧消息（GC 定时任务）。pg：删 outbox 行；nats：无 outbox，返回 0。
    pub async fn delete_delivered_older_than(&self, days: i64) -> Result<u64> {
        match self {
            #[cfg(feature = "pg")]
            Self::Pg(b) => b.delete_delivered_older_than(days).await,
            #[cfg(feature = "nats")]
            Self::Nats(_) => Ok(0),
        }
    }
}

//! PostgreSQL 后端（默认）：Outbox 表 `queues` + 进程内 dispatcher 轮询投递。
//!
//! - **入队**：与业务**同事务**写入 `queues` 表（`enqueue_event` 接收 `&mut PgConnection`）。
//! - **消费**：`run_dispatcher` 批事务轮询（FOR UPDATE SKIP LOCKED + SAVEPOINT + 指数退避），
//!   投递状态在 `queue_deliveries`（消息 × 监听者）。
//! - **GC**：`delete_delivered_older_than_in_transaction` 清理已投递消息。

use std::time::Duration;

use db::PgPool;
use rootcause::Result;
use sqlx::Acquire;
use tokio::sync::watch::Receiver;

use crate::event::Event;
use crate::registry::FrozenRegistry;
use crate::{dispatcher, enqueue, gc};

/// PG 后端句柄。
#[derive(Clone)]
pub struct PgBackend {
    pg_pool: PgPool,
}

impl PgBackend {
    pub fn new(pg_pool: PgPool) -> Self {
        Self { pg_pool }
    }
}

impl PgBackend {
    pub(crate) async fn enqueue_event<T: Event>(&self, event: &T) -> Result<()> {
        let mut conn = self.pg_pool.acquire().await?;
        enqueue::enqueue_event(&mut conn, event).await
    }

    pub(crate) async fn enqueue_event_delayed<T: Event>(
        &self,
        event: &T,
        delay: Duration,
    ) -> Result<()> {
        let mut conn = self.pg_pool.acquire().await?;
        enqueue::enqueue_event_with_delay(&mut conn, event, delay).await
    }

    pub(crate) async fn run_dispatcher<C: Send + Sync + 'static>(
        &self,
        ctx: C,
        registry: FrozenRegistry<C>,
        rx: Receiver<bool>,
    ) -> Result<()> {
        dispatcher::run_dispatcher(
            self.pg_pool.clone(),
            ctx,
            registry,
            dispatcher::DispatcherConfig::default(),
            rx,
        )
        .await
    }

    /// 清理已投递超过保留期的消息（含孤儿 inbox），供 GC 定时任务调用。
    pub(crate) async fn delete_delivered_older_than(&self, days: i64) -> Result<u64> {
        let mut conn = self.pg_pool.acquire().await?;
        let mut tx = conn.begin().await?;
        let deleted = gc::delete_delivered_older_than_in_transaction(&mut tx, days).await?;
        let deleted_inbox = gc::delete_orphaned_inbox_in_transaction(&mut tx).await?;
        tx.commit().await?;
        Ok(deleted + deleted_inbox)
    }
}

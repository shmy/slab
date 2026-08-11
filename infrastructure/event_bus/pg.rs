//! PostgreSQL 后端（默认）：Outbox 表 `events` + 进程内 dispatcher 轮询投递。
//!
//! - **发布**：后端自取连接写入 `events` 表（独立于调用方事务）。
//! - **消费**：`run_dispatcher` 批事务轮询（FOR UPDATE SKIP LOCKED + SAVEPOINT + 指数退避），
//!   投递状态在 `event_deliveries`（事件 × 订阅者）。
//! - **GC**：`delete_delivered_older_than_in_transaction` 清理已投递消息。
//! - **建表**：`try_new` 幂等自建全部队列表（`events` / `event_deliveries` + 索引 + 触发器），
//!   不依赖 migration 版本；使用运行时 `sqlx::query`（非宏），初次编译即可通过。

use std::time::Duration;

use crate::EventBacklog;
use db::PgPool;
use rootcause::Result;
use sqlx::{Acquire, PgConnection};
use tokio::sync::watch::Receiver;

use crate::event::Event;
use crate::registry::FrozenRegistry;
use crate::{dispatcher, gc, publish};

/// PG 后端句柄。
#[derive(Clone)]
pub struct PgBackend {
    pg_pool: PgPool,
}

impl PgBackend {
    /// 建立后端并跑事件总线迁移（`migrations/`，版本表 `_event_bus_migrations`）。
    /// 启动时执行（自愈：旧库自动升到最新），v1 保留幂等写法兼容迁移系统引入前的自建表。
    pub async fn try_new(pg_pool: PgPool) -> Result<Self> {
        // 迁移表名来自 sqlx.toml 的 table-name（编译期嵌入），与 sqlx CLI 保持一致。
        sqlx::migrate!("./migrations").run(&pg_pool).await?;
        Ok(Self { pg_pool })
    }
}

impl PgBackend {
    /// 积压统计（观测采样；`_pg_events` 状态 1=pending / 3=failed，投递表待投递数）。
    pub(crate) async fn backlog(&self) -> Result<EventBacklog> {
        use sqlx::Row as _;
        let row = sqlx::query(
            "SELECT count(*) FILTER (WHERE status = 1) AS pending,
                    count(*) FILTER (WHERE status = 3) AS failed
             FROM _pg_events",
        )
        .fetch_one(&self.pg_pool)
        .await?;
        let deliveries =
            sqlx::query("SELECT count(*) AS pending FROM _pg_event_deliveries WHERE status = 1")
                .fetch_one(&self.pg_pool)
                .await?;
        Ok(EventBacklog {
            pending: row.try_get("pending")?,
            failed: row.try_get("failed")?,
            deliveries_pending: deliveries.try_get("pending")?,
        })
    }

    pub(crate) async fn publish<T: Event>(&self, event: &T) -> Result<()> {
        let mut conn = self.pg_pool.acquire().await?;
        publish::publish(&mut conn, event).await
    }

    /// 事务内发布（强一致）：与业务同一事务，回滚即不投递（Outbox 语义）。
    pub(crate) async fn publish_in_tx<T: Event>(
        &self,
        executor: &mut PgConnection,
        event: &T,
    ) -> Result<()> {
        crate::publish::publish(executor, event).await
    }

    pub(crate) async fn publish_delayed<T: Event>(&self, event: &T, delay: Duration) -> Result<()> {
        let mut conn = self.pg_pool.acquire().await?;
        publish::publish_with_delay(&mut conn, event, delay).await
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

        tx.commit().await?;
        Ok(deleted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::Event;
    use serde::{Deserialize, Serialize};
    use sqlx::Row;

    #[derive(Debug, Serialize, Deserialize)]
    struct TestEvent {
        n: i32,
    }
    impl Event for TestEvent {
        const TOPIC: &'static str = "slab.pg_backend.evt";
    }

    /// backlog：空队列全零；发布后 pending 计数（delayed 未到期仍是 pending 状态）。
    #[sqlx::test]
    async fn backlog_counts_pending_events(pool: sqlx::PgPool) {
        let backend = PgBackend::try_new(pool.clone()).await.unwrap();
        assert_eq!(backend.backlog().await.unwrap(), EventBacklog::default());

        backend.publish(&TestEvent { n: 1 }).await.unwrap();
        backend.publish(&TestEvent { n: 2 }).await.unwrap();
        let backlog = backend.backlog().await.unwrap();
        assert_eq!(backlog.pending, 2);
        assert_eq!(backlog.failed, 0);
        assert_eq!(backlog.deliveries_pending, 0);
    }

    /// 不跑 migration：验证 `try_new` 幂等自建表（events + event_deliveries）后可直接入队。
    #[sqlx::test]
    async fn try_new_creates_schema_and_publishes(pool: sqlx::PgPool) {
        let backend = PgBackend::try_new(pool.clone()).await.unwrap();
        // 幂等：重复调用不报错（表/索引/触发器已存在）。
        PgBackend::try_new(pool.clone()).await.unwrap();

        backend.publish(&TestEvent { n: 7 }).await.unwrap();
        backend
            .publish_delayed(&TestEvent { n: 8 }, Duration::from_secs(60))
            .await
            .unwrap();

        let mut conn = pool.acquire().await.unwrap();
        let count = sqlx::query("SELECT COUNT(*) FROM _pg_events")
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        assert_eq!(count.get::<i64, _>("count"), 2);
    }
}

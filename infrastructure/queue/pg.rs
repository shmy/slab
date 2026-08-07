//! PostgreSQL 后端（默认）：Outbox 表 `queues` + 进程内 dispatcher 轮询投递。
//!
//! - **入队**：后端自取连接写入 `queues` 表（独立于调用方事务）。
//! - **消费**：`run_dispatcher` 批事务轮询（FOR UPDATE SKIP LOCKED + SAVEPOINT + 指数退避），
//!   投递状态在 `queue_deliveries`（消息 × 监听者）。
//! - **GC**：`delete_delivered_older_than_in_transaction` 清理已投递消息。
//! - **建表**：`try_new` 幂等自建全部队列表（`queues` / `queue_deliveries` + 索引 + 触发器），
//!   不依赖 migration 版本；使用运行时 `sqlx::query`（非宏），初次编译即可通过。

use std::time::Duration;

use db::PgPool;
use rootcause::Result;
use sqlx::{Acquire, PgConnection};
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
    /// 建立后端并幂等建表（`queues` / `queue_deliveries` + 索引 + 触发器）。
    pub async fn try_new(pg_pool: PgPool) -> Result<Self> {
        let mut conn = pg_pool.acquire().await?;
        sqlx::query(
            r#"
            CREATE OR REPLACE FUNCTION fn_set_updated_at()
            RETURNS TRIGGER AS $$
            BEGIN
              NEW.updated_at = CURRENT_TIMESTAMP;
              RETURN NEW;
            END;
            $$ LANGUAGE plpgsql
            "#,
        )
        .execute(&mut *conn)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS _pg_queues (
                id BIGSERIAL PRIMARY KEY,
                topic VARCHAR(255) NOT NULL,
                payload TEXT NOT NULL,
                -- 1=pending, 2=delivered, 3=failed
                status SMALLINT NOT NULL DEFAULT 1 CHECK (status IN (1, 2, 3)),
                delivered_at TIMESTAMPTZ,
                attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
                max_attempts INTEGER NOT NULL DEFAULT 5 CHECK (max_attempts > 0),
                next_attempt_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
                last_error TEXT,
                created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
                updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
            )
            "#,
        )
        .execute(&mut *conn)
        .await?;
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_pg_queues_pending ON _pg_queues (next_attempt_at, id) WHERE status = 1 AND attempts < max_attempts",
        )
        .execute(&mut *conn)
        .await?;
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_pg_queues_topic_pending ON _pg_queues (topic, next_attempt_at, id) WHERE status = 1 AND attempts < max_attempts",
        )
        .execute(&mut *conn)
        .await?;
        // CREATE TRIGGER 无 IF NOT EXISTS：DO 块内查 pg_trigger 幂等。
        sqlx::query(
            r#"
            DO $$ BEGIN
                IF NOT EXISTS (SELECT 1 FROM pg_trigger WHERE tgname = 'set_updated_at_queues') THEN
                    CREATE TRIGGER set_updated_at_queues BEFORE UPDATE ON _pg_queues
                    FOR EACH ROW EXECUTE PROCEDURE fn_set_updated_at();
                END IF;
            END $$
            "#,
        )
        .execute(&mut *conn)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS _pg_queue_deliveries (
                message_id      BIGINT NOT NULL REFERENCES _pg_queues(id) ON DELETE CASCADE,
                handler         TEXT NOT NULL,
                -- 1=pending, 2=delivered, 3=failed
                status          SMALLINT NOT NULL DEFAULT 1 CHECK (status IN (1, 2, 3)),
                attempts        INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
                max_attempts    INTEGER NOT NULL DEFAULT 5 CHECK (max_attempts > 0),
                next_attempt_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
                last_error      TEXT,
                delivered_at    TIMESTAMPTZ,
                created_at      TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
                updated_at      TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
                PRIMARY KEY (message_id, handler)
            )
            "#,
        )
        .execute(&mut *conn)
        .await?;
        sqlx::query(
            r#"
            DO $$ BEGIN
                IF NOT EXISTS (SELECT 1 FROM pg_trigger WHERE tgname = 'set_updated_at_queue_deliveries') THEN
                    CREATE TRIGGER set_updated_at_queue_deliveries BEFORE UPDATE ON _pg_queue_deliveries
                    FOR EACH ROW EXECUTE PROCEDURE fn_set_updated_at();
                END IF;
            END $$
            "#,
        )
        .execute(&mut *conn)
        .await?;
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_queue_deliveries_pending ON _pg_queue_deliveries (next_attempt_at, message_id) WHERE status = 1 AND attempts < max_attempts",
        )
        .execute(&mut *conn)
        .await?;
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_queue_deliveries_delivered ON _pg_queue_deliveries (delivered_at) WHERE status = 2 AND delivered_at IS NOT NULL",
        )
        .execute(&mut *conn)
        .await?;

        Ok(Self { pg_pool })
    }
}

impl PgBackend {
    pub(crate) async fn enqueue_event<T: Event>(&self, event: &T) -> Result<()> {
        let mut conn = self.pg_pool.acquire().await?;
        enqueue::enqueue_event(&mut conn, event).await
    }

    /// 事务内入队（强一致）：与业务同一事务，回滚即不投递（Outbox 语义）。
    pub(crate) async fn enqueue_event_in_tx<T: Event>(
        &self,
        executor: &mut PgConnection,
        event: &T,
    ) -> Result<()> {
        crate::enqueue::enqueue_event(executor, event).await
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

    /// 不跑 migration：验证 `try_new` 幂等自建表（queues + queue_deliveries）后可直接入队。
    #[sqlx::test]
    async fn try_new_creates_schema_and_enqueues(pool: sqlx::PgPool) {
        let backend = PgBackend::try_new(pool.clone()).await.unwrap();
        // 幂等：重复调用不报错（表/索引/触发器已存在）。
        PgBackend::try_new(pool.clone()).await.unwrap();

        backend.enqueue_event(&TestEvent { n: 7 }).await.unwrap();
        backend
            .enqueue_event_delayed(&TestEvent { n: 8 }, Duration::from_secs(60))
            .await
            .unwrap();

        let mut conn = pool.acquire().await.unwrap();
        let count = sqlx::query("SELECT COUNT(*) FROM _pg_queues")
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        assert_eq!(count.get::<i64, _>("count"), 2);
    }
}

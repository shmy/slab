//! 消费者幂等去重：`queue_inbox` 表 + `ensure_once`。
//!
//! **已废弃**：广播改造后去重职责由 `queue_deliveries` 表的主键 `(message_id, handler)` 承担
//! （dispatcher 不再调用本模块）。保留仅为兼容历史数据与潜在外部引用，新代码勿用。
//!
//! 每条队列消息（由 `queues.id` 标识）对每个 handler 最多处理一次。
//! 用法：handler 在执行业务逻辑前调用 `PgInbox::ensure_once(tx, message_id, handler_name)`。
//! 返回 `true` 表示是首次处理（须执行业务逻辑），`false` 表示已处理过（幂等跳过）。

use rootcause::Result;
use sqlx::{Executor, Postgres};

pub struct PgInbox;

impl PgInbox {
    /// 检查消息是否已被当前 handler 处理过，并原子标记为已处理。
    ///
    /// 返回 `true`：首次处理，调用方应执行 handler 业务逻辑。
    /// 返回 `false`：已处理过（幂等跳过），调用方应直接返回 `Ok(())`。
    ///
    /// 依赖 `queue_inbox` 的 `PRIMARY KEY (message_id, handler)` 保证唯一性。
    /// 适合在 dispatcher 的 handler 事务内调用（与业务写同事务）。
    pub async fn ensure_once<'e, E>(executor: E, message_id: i64, handler: &str) -> Result<bool>
    where
        E: Executor<'e, Database = Postgres>,
    {
        let result = sqlx::query(
            "INSERT INTO queue_inbox (message_id, handler) VALUES ($1, $2) ON CONFLICT DO NOTHING",
        )
        .bind(message_id)
        .bind(handler)
        .execute(executor)
        .await?;
        Ok(result.rows_affected() > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::PgInbox;
    use sqlx::Connection as _;

    #[sqlx::test]
    async fn ensure_once_returns_true_on_first_call(pool: sqlx::PgPool) -> sqlx::Result<()> {
        // Temp table 是连接级别的，需在同一连接上创建和使用
        let mut conn = pool.acquire().await?;
        sqlx::query(
            r#"
            CREATE TEMPORARY TABLE queue_inbox (
                message_id BIGINT NOT NULL,
                handler TEXT NOT NULL,
                processed_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
                PRIMARY KEY (message_id, handler)
            )
            "#,
        )
        .execute(&mut *conn)
        .await?;

        let mut tx = conn.begin().await?;
        let first = PgInbox::ensure_once(&mut *tx, 1, "test_handler")
            .await
            .unwrap();
        assert!(first, "first call should return true");

        let second = PgInbox::ensure_once(&mut *tx, 1, "test_handler")
            .await
            .unwrap();
        assert!(
            !second,
            "second call should return false (already processed)"
        );

        tx.commit().await?;
        Ok(())
    }
}

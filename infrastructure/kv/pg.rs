//! PostgreSQL `caches`（UNLOGGED）后端：默认后端，保留原始语义（可丢、TTL 判活、`take` 原子 DELETE）。
//!
//! 注意：每次操作从池独立取连接，**不参与调用方 PG 事务**——缓存是可丢辅助数据，
//! 调用方在业务事务提交后写入（顺序约定见 `docs/KV.md`）。
//!
//! 使用运行时 `sqlx::query`（非 `query!` 宏）：DDL/查询不依赖编译期数据库连接或 offline 数据，
//! 保证初次编译即可通过。

use std::time::Duration;

use chrono::{DateTime, Utc};
use db::PgPool;
use rootcause::Result;
use sqlx::Row;

/// 独立连接模式的 PG 后端。
#[derive(Clone)]
pub struct PgCache {
    pool: PgPool,
}

impl PgCache {
    /// 供 `KvBackend::try_new_pg` 调用；跑缓存迁移（`migrations/`，版本表 `_kv_migrations`）。
    /// 启动时执行（自愈：旧库自动升到最新），v1 保留幂等写法兼容迁移系统引入前的自建表。
    #[cfg(feature = "pg")]
    pub(crate) async fn try_new(pool: PgPool) -> Result<Self> {
        // 迁移表名来自 sqlx.toml 的 table-name（编译期嵌入），与 sqlx CLI 保持一致。
        sqlx::migrate!("./migrations").run(&pool).await?;
        Ok(Self { pool })
    }

    pub async fn get_raw(&self, key: &str) -> Result<Option<String>> {
        let mut conn = self.pool.acquire().await?;
        let row = sqlx::query("SELECT value FROM _pg_caches WHERE key = $1 AND expires_at > now()")
            .bind(key)
            .fetch_optional(&mut *conn)
            .await?;
        Ok(row.map(|row| row.get::<String, _>("value")))
    }

    pub async fn set_ex_raw(&self, key: &str, value: &str, period: Duration) -> Result<()> {
        let mut conn = self.pool.acquire().await?;
        let expires: DateTime<Utc> = Utc::now() + period;
        sqlx::query(
            r#"
            INSERT INTO _pg_caches (key, value, expires_at)
            VALUES ($1, $2, $3)
            ON CONFLICT (key) DO UPDATE
            SET value = EXCLUDED.value,
                expires_at = EXCLUDED.expires_at
            "#,
        )
        .bind(key)
        .bind(value)
        .bind(expires)
        .execute(&mut *conn)
        .await?;
        Ok(())
    }

    pub async fn take_raw(&self, key: &str) -> Result<Option<String>> {
        let mut conn = self.pool.acquire().await?;
        let row = sqlx::query(
            r#"
            DELETE FROM _pg_caches
            WHERE key = $1 AND expires_at > now()
            RETURNING value
            "#,
        )
        .bind(key)
        .fetch_optional(&mut *conn)
        .await?;
        Ok(row.map(|row| row.get::<String, _>("value")))
    }

    pub async fn del_raw(&self, key: &str) -> Result<()> {
        let mut conn = self.pool.acquire().await?;
        sqlx::query("DELETE FROM _pg_caches WHERE key = $1")
            .bind(key)
            .execute(&mut *conn)
            .await?;
        Ok(())
    }

    pub async fn delete_expired(&self) -> Result<u64> {
        let mut conn = self.pool.acquire().await?;
        // 无 advisory lock：多实例并发清理无害（DELETE 条件幂等）。
        let n = sqlx::query("DELETE FROM _pg_caches WHERE expires_at < now()")
            .execute(&mut *conn)
            .await?;
        Ok(n.rows_affected())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试本地建表（测试直接构造 PgCache 不走 try_new，故独立建表；IF NOT EXISTS 与迁移共存无害）。
    async fn ensure_schema(pool: &sqlx::PgPool) {
        let mut conn = pool.acquire().await.unwrap();
        sqlx::query(
            r#"
            CREATE UNLOGGED TABLE IF NOT EXISTS _pg_caches (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL,
                expires_at TIMESTAMPTZ NOT NULL
            )
            "#,
        )
        .execute(&mut *conn)
        .await
        .unwrap();
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_pg_caches_expires_at ON _pg_caches (expires_at)",
        )
        .execute(&mut *conn)
        .await
        .unwrap();
    }

    #[sqlx::test]
    async fn set_get_take_del(pool: sqlx::PgPool) {
        ensure_schema(&pool).await;
        let cache = PgCache { pool };

        assert!(cache.get_raw("k").await.unwrap().is_none());

        cache
            .set_ex_raw("k", "\"v1\"", Duration::from_secs(60))
            .await
            .unwrap();
        assert_eq!(cache.get_raw("k").await.unwrap().as_deref(), Some("\"v1\""));

        // take 原子消费：一次取走，第二次无。
        assert_eq!(
            cache.take_raw("k").await.unwrap().as_deref(),
            Some("\"v1\"")
        );
        assert!(cache.take_raw("k").await.unwrap().is_none());
        assert!(cache.get_raw("k").await.unwrap().is_none());

        cache
            .set_ex_raw("k", "\"v2\"", Duration::from_secs(60))
            .await
            .unwrap();
        cache.del_raw("k").await.unwrap();
        assert!(cache.get_raw("k").await.unwrap().is_none());
    }

    #[sqlx::test]
    async fn expired_is_invisible_and_cleaned(pool: sqlx::PgPool) {
        ensure_schema(&pool).await;
        let cache = PgCache { pool };

        cache
            .set_ex_raw("k1", "\"x\"", Duration::ZERO)
            .await
            .unwrap();
        cache
            .set_ex_raw("k2", "\"y\"", Duration::from_secs(60))
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(5)).await;

        assert!(cache.get_raw("k1").await.unwrap().is_none());
        assert_eq!(cache.get_raw("k2").await.unwrap().as_deref(), Some("\"y\""));

        let n = cache.delete_expired().await.unwrap();
        assert_eq!(n, 1);
        assert_eq!(cache.delete_expired().await.unwrap(), 0);
    }
}

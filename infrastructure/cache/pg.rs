//! PostgreSQL `caches`（UNLOGGED）后端：默认后端，保留原始语义（可丢、TTL 判活、`take` 原子 DELETE）。
//!
//! 注意：每次操作从池独立取连接，**不参与调用方 PG 事务**——缓存是可丢辅助数据，
//! 调用方在业务事务提交后写入（顺序约定见 `docs/CACHE.md`）。

use std::time::Duration;

use chrono::{DateTime, Utc};
use db::PgPool;
use rootcause::Result;

/// 独立连接模式的 PG 后端。
#[derive(Clone)]
pub struct PgCache {
    pool: PgPool,
}

impl PgCache {
    /// 仅当 pg 为唯一后端时被 `Backend::try_new` 调用（并集下 pg 让位，避免 dead code）。
    #[cfg(not(any(feature = "redb", feature = "redis")))]
    pub(crate) async fn try_new(pool: PgPool) -> Result<Self> {
        let mut txn = pool.begin().await?;
        sqlx::query!(
            r#"
CREATE UNLOGGED TABLE IF NOT EXISTS caches (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL
);
        "#
        )
        .execute(&mut *txn)
        .await?;
        sqlx::query!(
            r#"
CREATE INDEX IF NOT EXISTS idx_caches_expires_at ON caches (expires_at);
        "#
        )
        .execute(&mut *txn)
        .await?;
        txn.commit().await?;
        Ok(Self { pool })
    }

    pub async fn get_raw(&self, key: &str) -> Result<Option<String>> {
        let mut conn = self.pool.acquire().await?;
        let row = sqlx::query!(
            r#"SELECT value FROM caches WHERE key = $1 AND expires_at > now()"#,
            key,
        )
        .fetch_optional(&mut *conn)
        .await?;
        Ok(row.map(|row| row.value))
    }

    pub async fn set_ex_raw(&self, key: &str, value: &str, period: Duration) -> Result<()> {
        let mut conn = self.pool.acquire().await?;
        let expires: DateTime<Utc> = Utc::now() + period;
        sqlx::query!(
            r#"
            INSERT INTO caches (key, value, expires_at)
            VALUES ($1, $2, $3)
            ON CONFLICT (key) DO UPDATE
            SET value = EXCLUDED.value,
                expires_at = EXCLUDED.expires_at
            "#,
            key,
            value,
            &expires,
        )
        .execute(&mut *conn)
        .await?;
        Ok(())
    }

    pub async fn take_raw(&self, key: &str) -> Result<Option<String>> {
        let mut conn = self.pool.acquire().await?;
        let row = sqlx::query!(
            r#"
            DELETE FROM caches
            WHERE key = $1 AND expires_at > now()
            RETURNING value
            "#,
            key,
        )
        .fetch_optional(&mut *conn)
        .await?;
        Ok(row.map(|row| row.value))
    }

    pub async fn del_raw(&self, key: &str) -> Result<()> {
        let mut conn = self.pool.acquire().await?;
        sqlx::query!("DELETE FROM caches WHERE key = $1", key)
            .execute(&mut *conn)
            .await?;
        Ok(())
    }

    pub async fn delete_expired(&self) -> Result<u64> {
        let mut conn = self.pool.acquire().await?;
        // 无 advisory lock：多实例并发清理无害（DELETE 条件幂等）。
        let n = sqlx::query!("DELETE FROM caches WHERE expires_at < now()")
            .execute(&mut *conn)
            .await?;
        Ok(n.rows_affected())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use migration::run_migrations;

    #[sqlx::test]
    async fn set_get_take_del(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        // 直接构造：建表由 run_migrations 覆盖，无需 try_new（并集下可能被 cfg 禁用）。
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
        run_migrations(&pool).await.expect("run migrations");
        // 直接构造：建表由 run_migrations 覆盖，无需 try_new（并集下可能被 cfg 禁用）。
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

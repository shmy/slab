//! Redis 后端（bb8 连接池）。
//!
//! - **TTL**：Redis 原生过期，无需清扫（`delete_expired` 返回 0）。
//! - **原子 take**：`GETDEL`（Redis ≥ 6.2）一次性取出并删除。
//! - **共享**：多实例天然共享，适合跨实例会话/吊销场景。

use std::time::Duration;

use bb8_redis::RedisConnectionManager;
use bb8_redis::bb8::Pool;
use redis::AsyncCommands;
use rootcause::Result;

/// bb8 连接池后端。
#[derive(Clone)]
pub struct RedisCache {
    pool: Pool<RedisConnectionManager>,
}

impl RedisCache {
    /// 建立连接池。URL 形如 `redis://127.0.0.1:6379`。
    pub(crate) async fn try_new(url: &str) -> Result<Self> {
        let manager = RedisConnectionManager::new(url)?;
        let pool = Pool::builder().max_size(16).build(manager).await?;
        Ok(Self { pool })
    }
}

impl RedisCache {
    pub async fn get_raw(&self, key: &str) -> Result<Option<String>> {
        let mut conn = self.pool.get().await?;
        Ok(conn.get(key).await?)
    }

    pub async fn set_ex_raw(&self, key: &str, value: &str, period: Duration) -> Result<()> {
        let mut conn = self.pool.get().await?;
        let _: () = conn.set_ex(key, value, period.as_secs()).await?;
        Ok(())
    }

    pub async fn take_raw(&self, key: &str) -> Result<Option<String>> {
        let mut conn = self.pool.get().await?;
        // GETDEL：Redis ≥ 6.2 原子消费。
        let value: Option<String> = redis::cmd("GETDEL")
            .arg(key)
            .query_async(&mut *conn)
            .await?;
        Ok(value)
    }

    pub async fn del_raw(&self, key: &str) -> Result<()> {
        let mut conn = self.pool.get().await?;
        let _: () = conn.del(key).await?;
        Ok(())
    }

    pub async fn delete_expired(&self) -> Result<u64> {
        // Redis 键过期由服务端处理，无需清扫。
        Ok(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 需要本地 Redis（默认跳过）：`REDIS_TEST_URL=redis://127.0.0.1:6379 cargo test -p cache --features redis`
    fn test_url() -> Option<String> {
        std::env::var("REDIS_TEST_URL").ok()
    }

    #[tokio::test]
    async fn set_get_take_del() {
        let Some(url) = test_url() else {
            eprintln!("REDIS_TEST_URL not set, skipping redis backend test");
            return;
        };
        let cache = RedisCache::try_new(&url).await.unwrap();
        let key = "slab_redis_test";

        cache.del_raw(key).await.unwrap();
        assert!(cache.get_raw(key).await.unwrap().is_none());

        cache
            .set_ex_raw(key, "\"v1\"", Duration::from_secs(60))
            .await
            .unwrap();
        assert_eq!(cache.get_raw(key).await.unwrap().as_deref(), Some("\"v1\""));

        assert_eq!(
            cache.take_raw(key).await.unwrap().as_deref(),
            Some("\"v1\"")
        );
        assert!(cache.take_raw(key).await.unwrap().is_none());

        cache.del_raw(key).await.unwrap();
    }
}

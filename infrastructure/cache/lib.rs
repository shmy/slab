//! 统一缓存后端：`Backend` 枚举 + 方法门面。
//!
//! 编译期按 feature 装配（可并存，AppCtx 组装处选择用哪个变体）：
//! - `PgCache`：feature `pg`（**默认**），PostgreSQL `caches` UNLOGGED 表
//! - `RedbCache`：feature `redb`，redb 4 嵌入式 KV（可丢：`Durability::None`）
//! - `RedisCache`：feature `redis`，bb8 连接池 + Redis（TTL 由 Redis 原生处理）
//!
//! 无 trait / 无 `dyn` / 无手动 Pin：`Backend` 内部 match 派发，方法签名稳定。

#[cfg(feature = "pg")]
mod pg;
#[cfg(feature = "redb")]
mod redb;
#[cfg(feature = "redis")]
mod redis;

#[cfg(feature = "redb")]
use std::path::Path;
use std::time::Duration;

use rootcause::Result;
use serde::{Serialize, de::DeserializeOwned};

#[cfg(feature = "pg")]
pub use pg::PgCache;
#[cfg(feature = "redb")]
pub use redb::RedbCache;
#[cfg(feature = "redis")]
pub use redis::RedisCache;

#[cfg(not(any(feature = "pg", feature = "redb", feature = "redis")))]
compile_error!("cache crate requires feature \"pg\", \"redb\" or \"redis\"");

/// 缓存后端句柄：克隆共享、方法即 API。
#[derive(Clone)]
pub enum Backend {
    #[cfg(feature = "pg")]
    Pg(PgCache),
    #[cfg(feature = "redb")]
    Redb(RedbCache),
    #[cfg(feature = "redis")]
    Redis(RedisCache),
}

fn decode<T: DeserializeOwned>(raw: Option<String>) -> Option<T> {
    raw.and_then(|r| serde_json::from_str(&r).ok())
}

impl Backend {
    /// 仅当无 `redb` / `redis` 时提供：避免与其它后端同名 `try_new` 在 feature 并集下重复定义。
    #[cfg(all(feature = "pg", not(any(feature = "redb", feature = "redis"))))]
    pub async fn try_new(pool: sqlx::PgPool) -> Result<Self> {
        Ok(Self::Pg(PgCache::try_new(pool).await?))
    }

    #[cfg(feature = "redb")]
    pub fn try_new(path: impl AsRef<Path>) -> Result<Self> {
        Ok(Self::Redb(RedbCache::try_new(path)?))
    }

    #[cfg(feature = "redis")]
    pub async fn try_new(url: &str) -> Result<Self> {
        Ok(Self::Redis(RedisCache::try_new(url).await?))
    }

    /// 读缓存；未命中、已过期或反序列化失败返回 `None`（不区分）。
    pub async fn get<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        let raw = match self {
            #[cfg(feature = "pg")]
            Self::Pg(inner) => inner.get_raw(key).await?,
            #[cfg(feature = "redb")]
            Self::Redb(inner) => inner.get_raw(key).await?,
            #[cfg(feature = "redis")]
            Self::Redis(inner) => inner.get_raw(key).await?,
        };
        Ok(decode(raw))
    }

    /// 写入缓存并刷新 TTL。
    pub async fn set_ex<T: Serialize + Send + Sync>(
        &self,
        key: &str,
        value: &T,
        period: Duration,
    ) -> Result<()> {
        let raw = serde_json::to_string(value)?;
        match self {
            #[cfg(feature = "pg")]
            Self::Pg(inner) => inner.set_ex_raw(key, &raw, period).await,
            #[cfg(feature = "redb")]
            Self::Redb(inner) => inner.set_ex_raw(key, &raw, period).await,
            #[cfg(feature = "redis")]
            Self::Redis(inner) => inner.set_ex_raw(key, &raw, period).await,
        }
    }

    /// 原子消费：未过期则取出并删除；不存在或已过期返回 `None`。
    pub async fn take<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        let raw = match self {
            #[cfg(feature = "pg")]
            Self::Pg(inner) => inner.take_raw(key).await?,
            #[cfg(feature = "redb")]
            Self::Redb(inner) => inner.take_raw(key).await?,
            #[cfg(feature = "redis")]
            Self::Redis(inner) => inner.take_raw(key).await?,
        };
        Ok(decode(raw))
    }

    /// 删除 key（不区分是否过期）。
    pub async fn del(&self, key: &str) -> Result<()> {
        match self {
            #[cfg(feature = "pg")]
            Self::Pg(inner) => inner.del_raw(key).await,
            #[cfg(feature = "redb")]
            Self::Redb(inner) => inner.del_raw(key).await,
            #[cfg(feature = "redis")]
            Self::Redis(inner) => inner.del_raw(key).await,
        }
    }

    /// 清理已过期条目，返回删除条数。无 TTL 机制的实现（Redis）返回 0。
    pub async fn delete_expired(&self) -> Result<u64> {
        match self {
            #[cfg(feature = "pg")]
            Self::Pg(inner) => inner.delete_expired().await,
            #[cfg(feature = "redb")]
            Self::Redb(inner) => inner.delete_expired().await,
            #[cfg(feature = "redis")]
            Self::Redis(inner) => inner.delete_expired().await,
        }
    }
}

#[cfg(all(test, feature = "redb"))]
mod tests {
    use super::*;

    fn test_backend() -> Backend {
        let dir = Box::leak(Box::new(tempfile::tempdir().expect("create temp dir")));
        Backend::Redb(RedbCache::try_new(dir.path().join("cache.redb")).expect("open redb cache"))
    }

    #[tokio::test]
    async fn generic_facade_roundtrip() {
        let kv = test_backend();
        let key = "facade";

        assert!(kv.get::<String>(key).await.unwrap().is_none());

        kv.set_ex(key, &"hello", Duration::from_secs(60))
            .await
            .unwrap();
        assert_eq!(
            kv.get::<String>(key).await.unwrap().as_deref(),
            Some("hello")
        );

        // take 原子消费 + 类型切换（同 key 换类型也安全）。
        assert_eq!(
            kv.take::<String>(key).await.unwrap().as_deref(),
            Some("hello")
        );
        assert!(kv.get::<String>(key).await.unwrap().is_none());

        kv.set_ex(key, &42_i64, Duration::from_secs(60))
            .await
            .unwrap();
        assert_eq!(kv.get::<i64>(key).await.unwrap(), Some(42));

        kv.del(key).await.unwrap();
        assert!(kv.get::<i64>(key).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn corrupt_value_decodes_to_none() {
        let kv = test_backend();
        // raw 层写入非法 JSON：门面 get 应静默返回 None（不区分坏数据与未命中）。
        let Backend::Redb(inner) = &kv else {
            unreachable!("test backend is redb")
        };
        inner
            .set_ex_raw("bad", "not-json", Duration::from_secs(60))
            .await
            .unwrap();
        assert!(kv.get::<String>("bad").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn delete_expired_via_facade() {
        let kv = test_backend();
        kv.set_ex(&"x", &1_i64, Duration::ZERO).await.unwrap();
        kv.set_ex(&"y", &2_i64, Duration::from_secs(60))
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(5)).await;

        assert_eq!(kv.delete_expired().await.unwrap(), 1);
        assert!(kv.get::<i64>("x").await.unwrap().is_none());
        assert_eq!(kv.get::<i64>("y").await.unwrap(), Some(2));
    }
}

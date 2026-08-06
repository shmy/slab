//! 统一缓存后端：`Backend` 枚举 + 方法门面。
//!
//! 编译期按 feature 装配（可并存，AppCtx 组装处选择用哪个变体）：
//! - `RedbCache`：feature `redb`，redb 4 嵌入式 KV（可丢：`Durability::None`）
//! - `RedisCache`：feature `redis`，bb8 连接池 + Redis（TTL 由 Redis 原生处理）
//!
//! 无 trait / 无 `dyn` / 无手动 Pin：`Backend` 内部 match 派发，方法签名稳定。

#[cfg(feature = "redb")]
mod redb;
#[cfg(feature = "redis")]
mod redis;

use std::time::Duration;

use rootcause::Result;
use serde::{Serialize, de::DeserializeOwned};

#[cfg(feature = "redb")]
pub use redb::RedbCache;
#[cfg(feature = "redis")]
pub use redis::RedisCache;

#[cfg(not(any(feature = "redb", feature = "redis")))]
compile_error!("cache crate requires feature \"redb\" or \"redis\"");

/// 缓存后端句柄：克隆共享、方法即 API。
#[derive(Clone)]
pub enum Backend {
    #[cfg(feature = "redb")]
    Redb(RedbCache),
    #[cfg(feature = "redis")]
    Redis(RedisCache),
}

fn decode<T: DeserializeOwned>(raw: Option<String>) -> Option<T> {
    raw.and_then(|r| serde_json::from_str(&r).ok())
}

impl Backend {
    /// 读缓存；未命中、已过期或反序列化失败返回 `None`（不区分）。
    pub async fn get<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        let raw = match self {
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
            #[cfg(feature = "redb")]
            Self::Redb(inner) => inner.set_ex_raw(key, &raw, period).await,
            #[cfg(feature = "redis")]
            Self::Redis(inner) => inner.set_ex_raw(key, &raw, period).await,
        }
    }

    /// 原子消费：未过期则取出并删除；不存在或已过期返回 `None`。
    pub async fn take<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        let raw = match self {
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
            #[cfg(feature = "redb")]
            Self::Redb(inner) => inner.del_raw(key).await,
            #[cfg(feature = "redis")]
            Self::Redis(inner) => inner.del_raw(key).await,
        }
    }

    /// 清理已过期条目，返回删除条数。无 TTL 机制的实现（Redis）返回 0。
    pub async fn delete_expired(&self) -> Result<u64> {
        match self {
            #[cfg(feature = "redb")]
            Self::Redb(inner) => inner.delete_expired().await,
            #[cfg(feature = "redis")]
            Self::Redis(inner) => inner.delete_expired().await,
        }
    }
}

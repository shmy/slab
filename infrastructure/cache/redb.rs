//! redb 4 嵌入式 KV 后端。
//!
//! - **可丢语义**：每次写事务 `Durability::None`（不 fsync），对齐原 `UNLOGGED` 表——崩溃可丢、重启可重建。
//! - **TTL 自建**：值封装 `Entry { value, expires_at }`，`get` 惰性判活，`delete_expired` 扫表清理。
//! - **原子 take**：get + remove 在同一写事务内（redb 写事务串行，天然原子）。
//! - **单进程限制**：redb 数据库文件禁止多进程并行打开；多实例部署时每实例一份文件
//!   （会话数据可丢，跨实例吊销语义见 `docs/PG_CACHE.md` 演进记录——届时由 Redis 后端承担）。

use std::{ops::Add, path::Path, sync::Arc, time::Duration};

use chrono::Utc;
use redb::{Database, Durability, ReadableDatabase, ReadableTable, TableDefinition};
use rootcause::Result;
use serde::{Deserialize, Serialize};

const TABLE: TableDefinition<&str, &str> = TableDefinition::new("caches");

/// 值封装：业务 JSON + 绝对过期时间（毫秒）。
#[derive(Debug, Serialize, Deserialize)]
struct Entry {
    value: String,
    expires_at: i64,
}

/// 嵌入式 KV 后端。
#[derive(Clone)]
pub struct RedbCache {
    db: Arc<Database>,
}

impl RedbCache {
    /// 打开（不存在则创建）redb 数据库文件，并预创建缓存表。路径由 AppCtx 组装处配置。
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let db = Arc::new(Database::create(path.as_ref())?);
        // 写事务 open_table 会在表不存在时创建；否则后续读事务会报 TableDoesNotExist。
        let mut write_txn = db.begin_write()?;
        write_txn.set_durability(Durability::None)?;
        write_txn.open_table(TABLE)?;
        write_txn.commit()?;
        Ok(Self { db })
    }
    fn now_ms() -> i64 {
        Utc::now().timestamp_millis()
    }
}

impl RedbCache {
    pub async fn get_raw(&self, key: &str) -> Result<Option<String>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(TABLE)?;
        let Some(guard) = table.get(key)? else {
            return Ok(None);
        };
        let entry: Entry = serde_json::from_str(guard.value())?;
        if entry.expires_at < Self::now_ms() {
            return Ok(None);
        }
        Ok(Some(entry.value))
    }

    pub async fn set_ex_raw(&self, key: &str, value: &str, period: Duration) -> Result<()> {
        let expires_at = Utc::now().add(period).timestamp_millis();
        let encoded = serde_json::to_string(&Entry {
            value: value.to_owned(),
            expires_at,
        })?;
        let mut write_txn = self.db.begin_write()?;
        write_txn.set_durability(Durability::None)?;
        {
            let mut table = write_txn.open_table(TABLE)?;
            table.insert(key, encoded.as_str())?;
        }
        write_txn.commit()?;
        Ok(())
    }

    pub async fn take_raw(&self, key: &str) -> Result<Option<String>> {
        let mut write_txn = self.db.begin_write()?;
        write_txn.set_durability(Durability::None)?;
        let mut taken: Option<String> = None;
        {
            let mut table = write_txn.open_table(TABLE)?;
            if let Some(guard) = table.get(key)? {
                let entry: Entry = serde_json::from_str(guard.value())?;
                if entry.expires_at >= Self::now_ms() {
                    taken = Some(entry.value);
                }
            }
            // guard 作用域已结束，可安全删除。
            table.remove(key)?;
        }
        write_txn.commit()?;
        Ok(taken)
    }

    pub async fn del_raw(&self, key: &str) -> Result<()> {
        let mut write_txn = self.db.begin_write()?;
        write_txn.set_durability(Durability::None)?;
        {
            let mut table = write_txn.open_table(TABLE)?;
            table.remove(key)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    pub async fn delete_expired(&self) -> Result<u64> {
        let now = Self::now_ms();
        let mut write_txn = self.db.begin_write()?;
        write_txn.set_durability(Durability::None)?;
        // 先收集过期 key 再删除：redb 迭代器持有页面借用，迭代中不可写。
        let mut expired = Vec::new();
        {
            let table = write_txn.open_table(TABLE)?;
            for item in table.iter()? {
                let (k, v) = item?;
                if let Ok(entry) = serde_json::from_str::<Entry>(v.value())
                    && entry.expires_at < now
                {
                    expired.push(k.value().to_owned());
                }
            }
        }
        let count = expired.len() as u64;
        {
            let mut table = write_txn.open_table(TABLE)?;
            for key in &expired {
                table.remove(key.as_str())?;
            }
        }
        write_txn.commit()?;
        Ok(count)
    }
}

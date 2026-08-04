//! PostgreSQL 上的 **UNLOGGED** 热点 KV：`caches`，带 TTL 与可选事务内读写。
//!
//! - **可丢语义**：`UNLOGGED` 表在崩溃后可能丢失未持久化写入；仅适合会话类数据（见 `docs/PG_CACHE.md`）。
//! - **GC**：`delete_expired_in_transaction` 使用专用 advisory lock，勿与 `pg_queue` GC 共用 key。
//! - **文档**：`docs/PG_CACHE.md`；AI 速查见 `.cursor/skills/rust-slab-backend/SKILL.md` §11。

use std::fmt::Debug;

use chrono::{DateTime, Utc};
use serde::{Serialize, de::DeserializeOwned};
use sqlx::PgConnection;

/// 与 `bin/server/background` 中 `cache_gc` 任务成对使用；同库上勿占用相同 advisory key。
const GC_ADVISORY_KEY_1: i32 = 884_422;
const GC_ADVISORY_KEY_2: i32 = 1;

/// 在**当前**事务内先拿 `pg_advisory_xact_lock`（多实例/多进程互斥、提交或回滚时自动释放），
/// 再删除所有 `expires_at < now()` 的行；`execute` 返回删除行数。
pub async fn delete_expired_in_transaction(conn: &mut PgConnection) -> rootcause::Result<u64> {
    sqlx::query!(
        "SELECT pg_advisory_xact_lock($1, $2)",
        &GC_ADVISORY_KEY_1,
        &GC_ADVISORY_KEY_2,
    )
    .fetch_one(&mut *conn)
    .await?;
    let n = sqlx::query!("DELETE FROM caches WHERE expires_at < now()")
        .execute(&mut *conn)
        .await?;
    Ok(n.rows_affected())
}

pub async fn set_ex<T>(
    conn: &mut PgConnection,
    key: &str,
    value: &T,
    ttl_secs: u64,
) -> rootcause::Result<()>
where
    T: Serialize + Debug + Send + Sync,
{
    let expires: DateTime<Utc> = Utc::now() + chrono::Duration::seconds(ttl_secs as i64);
    let value = serde_json::to_string(value)?;
    sqlx::query!(
        r#"
            INSERT INTO caches (key, value, expires_at)
            VALUES ($1, $2, $3)
            ON CONFLICT (key) DO UPDATE
            SET value = EXCLUDED.value,
                expires_at = EXCLUDED.expires_at
            "#,
        &key,
        &value,
        &expires,
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

pub async fn get<T>(conn: &mut PgConnection, key: &str) -> rootcause::Result<Option<T>>
where
    T: DeserializeOwned + Send + Sync,
{
    let row = sqlx::query!(
        r#"SELECT value FROM caches WHERE key = $1 AND expires_at > now()"#,
        &key,
    )
    .fetch_optional(&mut *conn)
    .await?;
    Ok(row
        .map(|row| row.value)
        .and_then(|value: String| serde_json::from_str(&value).ok()))
}

pub async fn take<T>(conn: &mut PgConnection, key: &str) -> rootcause::Result<Option<T>>
where
    T: DeserializeOwned + Send + Sync,
{
    let row = sqlx::query!(
        r#"
        DELETE FROM caches
        WHERE key = $1
          AND expires_at > now()
        RETURNING value
        "#,
        &key,
    )
    .fetch_optional(&mut *conn)
    .await?;
    Ok(row
        .map(|row| row.value)
        .and_then(|value: String| serde_json::from_str(&value).ok()))
}

pub async fn del(conn: &mut PgConnection, key: &str) -> rootcause::Result<()> {
    sqlx::query!(r#"DELETE FROM caches WHERE key = $1"#, &key,)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

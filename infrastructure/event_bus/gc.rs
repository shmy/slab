/// 与 `bin/server/background` 中队列 GC 成对使用；勿与 `pg_cache` 的 advisory key 重复。
const QUEUE_GC_ADVISORY_KEY_1: i32 = 884_423;
const QUEUE_GC_ADVISORY_KEY_2: i32 = 1;
/// 已投递行按 `delivered_at` 保留的最少天数（小于 1 时按 1 处理，避免误删过新数据）。
pub const DEFAULT_DELIVERED_RETENTION_DAYS: i64 = 30;

use rootcause::Result;
use sqlx::PgConnection;

use crate::status::DeliveryStatus;

/// 在**当前**事务内先拿 `pg_advisory_xact_lock`（与 `pg_cache` 不同 key，可同库并行 GC），
/// 再删除 `delivered_at` 早于 `NOW() - retain_days` 的已投递行；返回删除行数。
pub async fn delete_delivered_older_than_in_transaction(
    conn: &mut PgConnection,
    retain_days: i64,
) -> Result<u64> {
    let days = retain_days.max(1);
    sqlx::query("SELECT pg_advisory_xact_lock($1, $2)")
        .bind(QUEUE_GC_ADVISORY_KEY_1)
        .bind(QUEUE_GC_ADVISORY_KEY_2)
        .fetch_one(&mut *conn)
        .await?;
    let n = sqlx::query(
        r#"
            DELETE FROM _pg_queues
            WHERE status = $2
              AND delivered_at IS NOT NULL
              AND delivered_at < NOW() - ($1::bigint * interval '1 day')
            "#,
    )
    .bind(days)
    .bind(DeliveryStatus::Delivered.as_i16())
    .execute(&mut *conn)
    .await?;
    Ok(n.rows_affected())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::{Acquire, PgPool, Row};

    /// 插入一条指定状态的队列消息，delivered_at 可指定；返回 id。
    async fn seed_queue(
        pool: &PgPool,
        status: i16,
        delivered_at: Option<chrono::DateTime<chrono::Utc>>,
    ) -> i64 {
        let mut conn = pool.acquire().await.unwrap();
        let row = sqlx::query(
            r#"INSERT INTO _pg_queues (topic, payload, status, delivered_at)
               VALUES ('gc.test', '{}', $1, $2) RETURNING id"#,
        )
        .bind(status)
        .bind(delivered_at)
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        row.get::<i64, _>("id")
    }

    #[sqlx::test]
    async fn test_gc_deletes_old_delivered_keeps_recent(pool: PgPool) {
        crate::pg::PgBackend::try_new(pool.clone()).await.unwrap();
        let old = seed_queue(
            &pool,
            2,
            Some(chrono::Utc::now() - chrono::Duration::days(30)),
        )
        .await;
        let recent = seed_queue(&pool, 2, Some(chrono::Utc::now())).await;
        let pending = seed_queue(&pool, 1, None).await;

        let mut conn = pool.acquire().await.unwrap();
        let mut txn = conn.begin().await.unwrap();
        let n = delete_delivered_older_than_in_transaction(&mut txn, 7)
            .await
            .unwrap();
        txn.commit().await.unwrap();

        assert_eq!(n, 1);
        let remaining = sqlx::query("SELECT id FROM _pg_queues ORDER BY id")
            .fetch_all(&pool)
            .await
            .unwrap()
            .into_iter()
            .map(|r| r.get::<i64, _>("id"))
            .collect::<Vec<_>>();
        assert_eq!(remaining, vec![recent, pending]);
        assert!(!remaining.contains(&old));
    }

    #[sqlx::test]
    async fn test_gc_retain_days_floored_at_one(pool: PgPool) {
        crate::pg::PgBackend::try_new(pool.clone()).await.unwrap();
        let old = seed_queue(
            &pool,
            2,
            Some(chrono::Utc::now() - chrono::Duration::days(30)),
        )
        .await;
        let very_new = seed_queue(
            &pool,
            2,
            Some(chrono::Utc::now() - chrono::Duration::hours(2)),
        )
        .await;

        let mut conn = pool.acquire().await.unwrap();
        let mut txn = conn.begin().await.unwrap();
        // retain_days=0 按 1 天处理：30 天前的删除，2 小时前的保留
        let n = delete_delivered_older_than_in_transaction(&mut txn, 0)
            .await
            .unwrap();
        txn.commit().await.unwrap();

        assert_eq!(n, 1);
        let remaining = sqlx::query("SELECT id FROM _pg_queues")
            .fetch_all(&pool)
            .await
            .unwrap()
            .into_iter()
            .map(|r| r.get::<i64, _>("id"))
            .collect::<Vec<_>>();
        assert_eq!(remaining, vec![very_new]);
        assert!(!remaining.contains(&old));
    }
}

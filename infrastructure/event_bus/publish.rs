//! 与 dispatcher 匹配的入队：`_pg_events`，依赖表上 `status` 默认值（pending）。
use std::time::Duration;

use crate::event::Event;
use sqlx::PgConnection;
pub async fn publish<T: Event>(executor: &mut PgConnection, event: &T) -> rootcause::Result<()> {
    let payload_json = serde_json::to_string(event)?;
    sqlx::query("INSERT INTO _pg_events (topic, payload) VALUES ($1, $2)")
        .bind(T::TOPIC)
        .bind(&payload_json)
        .execute(executor)
        .await?;
    Ok(())
}

pub async fn publish_with_delay<T: Event>(
    executor: &mut PgConnection,
    event: &T,
    delay: Duration,
) -> rootcause::Result<()> {
    let payload_json = serde_json::to_string(event)?;
    sqlx::query(
        "INSERT INTO _pg_events (topic, payload, next_attempt_at) VALUES ($1, $2, NOW() + ($3 * interval '1 second'))",
    )
    .bind(T::TOPIC)
    .bind(&payload_json)
    .bind(delay.as_secs_f64())
    .execute(executor)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};
    use sqlx::Row;

    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct TestEvent {
        n: i32,
    }

    impl Event for TestEvent {
        const TOPIC: &'static str = "slab.test.publish";
    }

    #[sqlx::test]
    async fn test_publish_inserts_pending_row(pool: sqlx::PgPool) {
        crate::pg::PgBackend::try_new(pool.clone()).await.unwrap();
        let mut conn = pool.acquire().await.unwrap();

        publish(&mut *conn, &TestEvent { n: 7 }).await.unwrap();

        let row = sqlx::query("SELECT topic, payload, status, attempts FROM _pg_events")
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        assert_eq!(row.get::<String, _>("topic"), "slab.test.publish");
        assert_eq!(row.get::<i16, _>("status"), 1); // pending（依赖表默认值）
        assert_eq!(row.get::<i32, _>("attempts"), 0);
        assert!(row.get::<String, _>("payload").contains("\"n\":7"));
    }

    #[sqlx::test]
    async fn test_publish_with_delay_sets_future_attempt(pool: sqlx::PgPool) {
        crate::pg::PgBackend::try_new(pool.clone()).await.unwrap();
        let mut conn = pool.acquire().await.unwrap();

        publish_with_delay(&mut *conn, &TestEvent { n: 7 }, Duration::from_secs(60))
            .await
            .unwrap();

        // 延迟发布：next_attempt_at 必须晚于当前时刻
        let row = sqlx::query("SELECT (next_attempt_at > NOW()) AS is_future FROM _pg_events")
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        assert!(row.get::<bool, _>("is_future"));
    }
}

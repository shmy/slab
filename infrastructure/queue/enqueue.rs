//! 与 dispatcher 匹配的入队：`queues`，依赖表上 `status` 默认值（pending）。
use std::time::Duration;

use crate::event::Event;
use sqlx::{Executor, Postgres};
pub async fn enqueue_event<'e, E, T: Event>(executor: E, event: &T) -> rootcause::Result<()>
where
    E: Executor<'e, Database = Postgres>,
{
    let payload_json = serde_json::to_string(event)?;
    sqlx::query!(
        r#"
        INSERT INTO queues (topic, payload)
        VALUES ($1, $2)
        "#,
        T::TOPIC,
        payload_json,
    )
    .execute(executor)
    .await?;
    Ok(())
}

pub async fn enqueue_event_with_delay<'e, E, T: Event>(
    executor: E,
    event: &T,
    delay: Duration,
) -> rootcause::Result<()>
where
    E: Executor<'e, Database = Postgres>,
{
    let payload_json = serde_json::to_string(event)?;
    sqlx::query!(
        r#"
        INSERT INTO queues (topic, payload, next_attempt_at)
        VALUES ($1, $2, NOW() + ($3 * interval '1 second'))
        "#,
        T::TOPIC,
        payload_json,
        delay.as_secs_f64(),
    )
    .execute(executor)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use migration::run_migrations;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct TestEvent {
        n: i32,
    }

    impl Event for TestEvent {
        const TOPIC: &'static str = "slab.test.enqueue";
    }

    #[sqlx::test]
    async fn test_enqueue_event_inserts_pending_row(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let mut conn = pool.acquire().await.unwrap();

        enqueue_event(&mut *conn, &TestEvent { n: 7 })
            .await
            .unwrap();

        let row = sqlx::query!("SELECT topic, payload, status, attempts FROM queues")
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        assert_eq!(row.topic, "slab.test.enqueue");
        assert_eq!(row.status, 1); // pending（依赖表默认值）
        assert_eq!(row.attempts, 0);
        assert!(row.payload.contains("\"n\":7"));
    }

    #[sqlx::test]
    async fn test_enqueue_event_with_delay_sets_future_attempt(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let mut conn = pool.acquire().await.unwrap();

        enqueue_event_with_delay(&mut *conn, &TestEvent { n: 7 }, Duration::from_secs(60))
            .await
            .unwrap();

        // 延迟入队：next_attempt_at 必须晚于当前时刻
        let row = sqlx::query!("SELECT (next_attempt_at > NOW()) AS \"is_future!\" FROM queues")
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        assert!(row.is_future);
    }
}

use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use std::time::Duration;

use db::PgPool;
use futures_util::FutureExt;
use rootcause::{Result, report};
use sqlx::{FromRow, PgConnection};
use tokio::sync::watch::Receiver;

use crate::handler::QueueHandler;
use crate::registry::FrozenRegistry;
use crate::status::{QueueStatus, RetryNextAttempt, RetryPlan};

const DEFAULT_BACKOFF_MAX_SECS: i64 = 300;
const DEFAULT_BATCH_SIZE: i64 = 32;

/// 一条待处理消息（`_pg_queues` 行）。
#[derive(FromRow)]
struct MessageRow {
    id: i64,
    topic: String,
    payload: String,
}

/// 一条未完成的投递任务（`_pg_queue_deliveries` 行：消息 × 监听者）。
#[derive(FromRow)]
struct DeliveryRow {
    handler: String,
    attempts: i32,
    max_attempts: i32,
}

#[derive(Clone, Debug)]
pub struct DispatcherConfig {
    pub poll_interval: Duration,
    pub backoff_max_secs: i64,
    pub batch_size: i64,
}

impl Default for DispatcherConfig {
    fn default() -> Self {
        Self {
            poll_interval: Duration::from_secs(1),
            backoff_max_secs: DEFAULT_BACKOFF_MAX_SECS,
            batch_size: DEFAULT_BATCH_SIZE,
        }
    }
}

#[tracing::instrument(skip_all)]
pub async fn run_dispatcher<C: Send + Sync + 'static>(
    pg_pool: PgPool,
    ctx: C,
    registry: FrozenRegistry<C>,
    config: DispatcherConfig,
    mut shutdown: Receiver<bool>,
) -> Result<()> {
    loop {
        let processed = match run_one_cycle(&pg_pool, &ctx, &registry, &config).await {
            Ok(processed) => processed,
            Err(error) => {
                tracing::warn!(%error, "pg_queue dispatcher cycle failed");
                0
            }
        };
        if processed > 0 {
            if *shutdown.borrow() {
                tracing::info!("pg_queue dispatcher received shutdown signal");
                return Ok(());
            }
            continue;
        }
        tokio::select! {
            _ = wait_shutdown(&mut shutdown) => {
                tracing::info!("pg_queue dispatcher received shutdown signal");
                return Ok(());
            }
            _ = tokio::time::sleep(config.poll_interval) => {}
        }
    }
}

#[tracing::instrument(skip_all)]
async fn run_one_cycle<C: Send + Sync + 'static>(
    pg_pool: &PgPool,
    ctx: &C,
    registry: &FrozenRegistry<C>,
    config: &DispatcherConfig,
) -> Result<usize> {
    tracing::trace!("pg_queue dispatcher running one cycle");
    let mut tx = pg_pool.begin().await?;
    let batch_size = config.batch_size.max(1);
    // 拉取「有待处理投递」的消息行：
    //   - 从未生成过投递（旧消息/新消息，等待 ensure）或
    //   - 存在到期未完成的投递（status=pending 且 attempts 未耗尽且 next_attempt_at 已到）
    // 行级状态不做拉取条件（纯聚合/告警），pending 投递存在与否才是驱动力：
    // 部分失败 + 部分退避的消息行仍可被拉取，直到所有投递终态。
    let rows = sqlx::query_as::<_, MessageRow>(
        r#"
            SELECT q.id, q.topic, q.payload
            FROM _pg_queues q
            WHERE q.next_attempt_at <= NOW()
              AND (
                  NOT EXISTS (
                      SELECT 1 FROM _pg_queue_deliveries d
                      WHERE d.message_id = q.id
                  )
                  OR EXISTS (
                      SELECT 1 FROM _pg_queue_deliveries d
                      WHERE d.message_id = q.id
                        AND d.status = $1
                        AND d.attempts < d.max_attempts
                        AND d.next_attempt_at <= NOW()
                  )
              )
            ORDER BY q.next_attempt_at ASC, q.id ASC
            FOR UPDATE SKIP LOCKED
            LIMIT $2
            "#,
    )
    .bind(QueueStatus::Pending.as_i16())
    .bind(batch_size)
    .fetch_all(&mut *tx)
    .await?;

    if rows.is_empty() {
        tx.commit().await?;
        return Ok(0);
    }

    let processed = rows.len();
    for row in rows {
        process_message(&mut tx, ctx, registry, config, row).await?;
    }

    tx.commit().await?;
    Ok(processed)
}

/// 处理一条消息：确保所有监听者的投递行存在，逐个投递，最后聚合刷新消息行状态。
async fn process_message<C: Send + Sync + 'static>(
    tx: &mut PgConnection,
    ctx: &C,
    registry: &FrozenRegistry<C>,
    config: &DispatcherConfig,
    row: MessageRow,
) -> Result<()> {
    let id = row.id;
    let topic = row.topic;
    let payload: serde_json::Value = serde_json::from_str(&row.payload).map_err(|e| {
        report!("_pg_queues.payload is not valid JSON (queue_id={id}, topic={topic}): {e}")
    })?;

    let Some(handlers) = registry.get(topic.as_str()) else {
        tracing::warn!(
            queue_id = id,
            topic,
            "pg_queue: no handler for topic, mark as terminal failure"
        );
        mark_queues_terminal_failure(tx, id, format!("no_handler_for_topic:{topic}").as_str())
            .await?;
        return Ok(());
    };

    // 为当前注册的每个监听者生成投递行（新监听者上线后自动补上仍 pending 的消息）。
    ensure_deliveries(tx, id, handlers).await?;

    // 拉取该消息尚未完成的投递任务（严格按各自退避时间门控：
    // 一个监听者到期不牵连其它仍在退避中的监听者）。
    let deliveries = sqlx::query_as::<_, DeliveryRow>(
        r#"
            SELECT handler, attempts, max_attempts
            FROM _pg_queue_deliveries
            WHERE message_id = $1
              AND status = $2
              AND attempts < max_attempts
              AND next_attempt_at <= NOW()
            ORDER BY handler ASC
            "#,
    )
    .bind(id)
    .bind(QueueStatus::Pending.as_i16())
    .fetch_all(&mut *tx)
    .await?;

    for delivery in deliveries {
        // 按 name 找到对应监听者（注册表在运行中不会变，此分支仅为防御）。
        let Some(handler) = handlers.iter().find(|h| h.name() == delivery.handler) else {
            tracing::warn!(
                queue_id = id,
                handler = %delivery.handler,
                "pg_queue: delivery references unregistered handler, mark as terminal failure"
            );
            mark_delivery_terminal_failure(tx, id, &delivery.handler, "handler_not_registered")
                .await?;
            continue;
        };
        deliver_one(tx, ctx, handler, &payload, config, id, &delivery).await?;
    }

    refresh_message_state(tx, id).await?;
    Ok(())
}

/// 为 topic 的所有监听者补投递行（幂等：已有行不重复）。
async fn ensure_deliveries<C: Send + Sync + 'static>(
    tx: &mut PgConnection,
    id: i64,
    handlers: &[Arc<dyn QueueHandler<C>>],
) -> Result<()> {
    let names: Vec<String> = handlers.iter().map(|h| h.name().to_string()).collect();
    sqlx::query(
        r#"
        INSERT INTO _pg_queue_deliveries (message_id, handler, max_attempts)
        SELECT $1, h.handler, q.max_attempts
        FROM unnest($2::text[]) AS h(handler)
        CROSS JOIN _pg_queues q
        WHERE q.id = $1
        ON CONFLICT DO NOTHING
        "#,
    )
    .bind(id)
    .bind(&names)
    .execute(&mut *tx)
    .await?;
    Ok(())
}

/// 投递单个监听者：SAVEPOINT 隔离，成功标记 delivered，失败按退避计划重试/终态。
/// 一个监听者失败不影响其它监听者，也不影响同批其它消息。
async fn deliver_one<C: Send + Sync + 'static>(
    tx: &mut PgConnection,
    ctx: &C,
    handler: &Arc<dyn QueueHandler<C>>,
    payload: &serde_json::Value,
    config: &DispatcherConfig,
    id: i64,
    delivery: &DeliveryRow,
) -> Result<()> {
    let handler_name = &delivery.handler;
    let attempts = delivery.attempts;
    let max_attempts = delivery.max_attempts;

    sqlx::query("SAVEPOINT pg_queue_handler")
        .execute(&mut *tx)
        .await?;
    // 捕获订阅者 panic：handler 的 panic 不得传播到分发任务（否则会经
    // server 的 join_task resume_unwind 拖垮整个 HTTP 服务）。
    // panic 按终态失败记录（不重试），SAVEPOINT 回滚 handler 的部分写入。
    let handle_result = AssertUnwindSafe(handler.handle(ctx, payload.clone()))
        .catch_unwind()
        .await;
    match handle_result {
        Ok(Ok(())) => {
            sqlx::query("RELEASE SAVEPOINT pg_queue_handler")
                .execute(&mut *tx)
                .await?;
            mark_delivery_delivered(tx, id, handler_name).await?;
        }
        Ok(Err(error)) => {
            let message = error.to_string();
            sqlx::query("ROLLBACK TO SAVEPOINT pg_queue_handler")
                .execute(&mut *tx)
                .await?;
            record_delivery_retry(
                tx,
                id,
                handler_name,
                attempts,
                max_attempts,
                config,
                message.as_str(),
            )
            .await?;
            sqlx::query("RELEASE SAVEPOINT pg_queue_handler")
                .execute(&mut *tx)
                .await?;
        }
        Err(panic) => {
            let message = panic_message(&panic);
            sqlx::query("ROLLBACK TO SAVEPOINT pg_queue_handler")
                .execute(&mut *tx)
                .await?;
            mark_delivery_terminal_failure(tx, id, handler_name, &message).await?;
            sqlx::query("RELEASE SAVEPOINT pg_queue_handler")
                .execute(&mut *tx)
                .await?;
        }
    }

    Ok(())
}

/// 提取 panic 消息（支持 &str / String / 其他 payload）。
fn panic_message(payload: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        format!("handler panicked: {s}")
    } else if let Some(s) = payload.downcast_ref::<String>() {
        format!("handler panicked: {s}")
    } else {
        "handler panicked".to_string()
    }
}

async fn mark_delivery_delivered(tx: &mut PgConnection, id: i64, handler: &str) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE _pg_queue_deliveries
        SET status = $3,
            delivered_at = NOW(),
            last_error = NULL
        WHERE message_id = $1 AND handler = $2
        "#,
    )
    .bind(id)
    .bind(handler)
    .bind(QueueStatus::Delivered.as_i16())
    .execute(&mut *tx)
    .await?;
    Ok(())
}

async fn mark_delivery_terminal_failure(
    tx: &mut PgConnection,
    id: i64,
    handler: &str,
    error: &str,
) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE _pg_queue_deliveries
        SET status = $4,
            attempts = max_attempts,
            last_error = $3,
            next_attempt_at = 'infinity'::timestamptz
        WHERE message_id = $1 AND handler = $2
        "#,
    )
    .bind(id)
    .bind(handler)
    .bind(error)
    .bind(QueueStatus::Failed.as_i16())
    .execute(&mut *tx)
    .await?;
    Ok(())
}

/// 消息整体终态失败（无监听者订阅该 topic 时触发，保留防呆语义）。
async fn mark_queues_terminal_failure(tx: &mut PgConnection, id: i64, error: &str) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE _pg_queues
        SET status = $3,
            attempts = max_attempts,
            last_error = $2,
            next_attempt_at = 'infinity'::timestamptz
        WHERE id = $1
        "#,
    )
    .bind(id)
    .bind(error)
    .bind(QueueStatus::Failed.as_i16())
    .execute(&mut *tx)
    .await?;
    Ok(())
}

async fn record_delivery_retry(
    tx: &mut PgConnection,
    id: i64,
    handler: &str,
    attempts: i32,
    max_attempts: i32,
    config: &DispatcherConfig,
    error: &str,
) -> Result<()> {
    let plan = RetryPlan::from_failure(attempts, max_attempts, config.backoff_max_secs, error);
    match plan.next_attempt_at {
        RetryNextAttempt::Terminal => {
            sqlx::query(
                r#"
                UPDATE _pg_queue_deliveries
                SET status = $4,
                    attempts = $2,
                    last_error = $3,
                    next_attempt_at = 'infinity'::timestamptz
                WHERE message_id = $1 AND handler = $5
                "#,
            )
            .bind(id)
            .bind(plan.attempts)
            .bind(&plan.last_error)
            .bind(plan.status.as_i16())
            .bind(handler)
            .execute(&mut *tx)
            .await?;
        }
        RetryNextAttempt::DelaySecs(delay_secs) => {
            sqlx::query(
                r#"
                UPDATE _pg_queue_deliveries
                SET status = $5,
                    attempts = $2,
                    last_error = $3,
                    next_attempt_at = NOW() + ($4::bigint * interval '1 second')
                WHERE message_id = $1 AND handler = $6
                "#,
            )
            .bind(id)
            .bind(plan.attempts)
            .bind(&plan.last_error)
            .bind(delay_secs)
            .bind(plan.status.as_i16())
            .bind(handler)
            .execute(&mut *tx)
            .await?;
        }
    }
    Ok(())
}

/// 聚合所有投递行的状态，刷新消息行（**终态失败优先**，保证告警即时可见）：
/// - 存在终态失败投递 → 行 failed（last_error 汇总最近一条）；仍有 pending 投递时
///   行 next_attempt_at 保持为最早未完成投递时间，拉取仍会继续（行状态纯告警语义）
/// - 无失败但有 pending → 行 pending，next_attempt_at = 最早未完成投递时间
/// - 全部成功 → 行 delivered（GC 依据 delivered_at 清理）
/// - 全部终态后 next_attempt_at = infinity，行不再被拉取
///
/// 人工修复路径：将失败投递行改回 pending 并重置 attempts/next_attempt_at，
/// 同时把消息行 next_attempt_at 拨回过去（行状态由下次处理自动刷新）。
async fn refresh_message_state(tx: &mut PgConnection, id: i64) -> Result<()> {
    sqlx::query(
        r#"
        WITH agg AS (
            SELECT
                COUNT(*) FILTER (WHERE status = $2) AS pending,
                COUNT(*) FILTER (WHERE status = $3) AS failed
            FROM _pg_queue_deliveries
            WHERE message_id = $1
        )
        UPDATE _pg_queues q
        SET status = CASE
                WHEN agg.failed > 0 THEN $3
                WHEN agg.pending > 0 THEN $2
                ELSE $4
            END,
            delivered_at = CASE
                WHEN agg.pending = 0 AND agg.failed = 0 THEN NOW()
                ELSE delivered_at
            END,
            next_attempt_at = CASE
                WHEN agg.pending > 0 THEN (
                    SELECT MIN(d.next_attempt_at)
                    FROM _pg_queue_deliveries d
                    WHERE d.message_id = q.id AND d.status = $2
                )
                ELSE 'infinity'::timestamptz
            END,
            last_error = CASE
                WHEN agg.failed > 0 THEN COALESCE(
                    (SELECT d.last_error
                     FROM _pg_queue_deliveries d
                     WHERE d.message_id = q.id AND d.status = $3
                     ORDER BY d.updated_at DESC
                     LIMIT 1),
                    'some_deliveries_failed'
                )
                ELSE last_error
            END
        FROM agg
        WHERE q.id = $1
        "#,
    )
    .bind(id)
    .bind(QueueStatus::Pending.as_i16())
    .bind(QueueStatus::Failed.as_i16())
    .bind(QueueStatus::Delivered.as_i16())
    .execute(&mut *tx)
    .await?;
    Ok(())
}

async fn wait_shutdown(shutdown: &mut Receiver<bool>) {
    if *shutdown.borrow() {
        return;
    }
    let _ = shutdown.changed().await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::Registry;
    use serde_json::{Value, json};
    use sqlx::{PgPool, Row};
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// 测试监听者：统计调用次数；`fail_until` 次调用内返回 Err，之后成功。
    struct TestHandler {
        topic: &'static str,
        name: &'static str,
        calls: Arc<AtomicUsize>,
        fail_until: usize,
    }

    impl TestHandler {
        fn ok(topic: &'static str, name: &'static str) -> Self {
            Self {
                topic,
                name,
                calls: Arc::new(AtomicUsize::new(0)),
                fail_until: 0,
            }
        }
        fn failing(topic: &'static str, name: &'static str) -> Self {
            Self {
                topic,
                name,
                calls: Arc::new(AtomicUsize::new(0)),
                fail_until: usize::MAX,
            }
        }
        fn flaky(topic: &'static str, name: &'static str, fail_until: usize) -> Self {
            Self {
                topic,
                name,
                calls: Arc::new(AtomicUsize::new(0)),
                fail_until,
            }
        }
    }

    impl<C: Send + Sync + 'static> QueueHandler<C> for TestHandler {
        fn topic(&self) -> &'static str {
            self.topic
        }
        fn name(&self) -> &'static str {
            self.name
        }
        fn handle<'a>(
            &'a self,
            _ctx: &'a C,
            _payload: Value,
        ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
            let attempt = self.calls.fetch_add(1, Ordering::SeqCst);
            if attempt < self.fail_until {
                Box::pin(async { Err(report!("boom")) })
            } else {
                Box::pin(async { Ok(()) })
            }
        }
    }

    async fn enqueue(pool: &PgPool, topic: &str, max_attempts: i32) -> i64 {
        sqlx::query(
            "INSERT INTO _pg_queues (topic, payload, max_attempts) VALUES ($1, $2, $3) RETURNING id",
        )
        .bind(topic)
        .bind(serde_json::to_string(&json!({"n": 1})).unwrap())
        .bind(max_attempts)
        .fetch_one(pool)
        .await
        .unwrap()
        .get::<i64, _>("id")
    }

    async fn delivery_status(pool: &PgPool, message_id: i64, handler: &str) -> (i16, i32) {
        let row = sqlx::query(
            "SELECT status, attempts FROM _pg_queue_deliveries WHERE message_id = $1 AND handler = $2",
        )
        .bind(message_id)
        .bind(handler)
        .fetch_one(pool)
        .await
        .unwrap();
        (row.get::<i16, _>("status"), row.get::<i32, _>("attempts"))
    }

    async fn queue_status(pool: &PgPool, id: i64) -> i16 {
        sqlx::query("SELECT status FROM _pg_queues WHERE id = $1")
            .bind(id)
            .fetch_one(pool)
            .await
            .unwrap()
            .get::<i16, _>("status")
    }

    fn config() -> DispatcherConfig {
        DispatcherConfig {
            poll_interval: Duration::from_millis(10),
            backoff_max_secs: 60,
            batch_size: 32,
        }
    }

    #[test]
    fn dispatcher_defaults_to_batch_claiming() {
        assert_eq!(DispatcherConfig::default().batch_size, 32);
    }

    #[sqlx::test]
    async fn broadcast_delivers_to_every_handler_of_topic(pool: PgPool) {
        crate::pg::PgBackend::try_new(pool.clone()).await.unwrap();
        let handler_a = TestHandler::ok("slab.test.evt", "listener_a");
        let handler_b = TestHandler::ok("slab.test.evt", "listener_b");
        let calls_a = handler_a.calls.clone();
        let calls_b = handler_b.calls.clone();
        let mut registry = Registry::<()>::default();
        registry.register(handler_a).register(handler_b);
        let registry = registry.freeze();

        let id = enqueue(&pool, "slab.test.evt", 5).await;

        let processed = run_one_cycle(&pool, &(), &registry, &config())
            .await
            .unwrap();
        assert_eq!(processed, 1);
        assert_eq!(calls_a.load(Ordering::SeqCst), 1);
        assert_eq!(calls_b.load(Ordering::SeqCst), 1);
        assert_eq!(delivery_status(&pool, id, "listener_a").await, (2, 0));
        assert_eq!(delivery_status(&pool, id, "listener_b").await, (2, 0));
        assert_eq!(queue_status(&pool, id).await, 2); // delivered
    }

    /// 测试监听者：handle 中直接 panic。
    struct PanicHandler;

    impl<C: Send + Sync + 'static> QueueHandler<C> for PanicHandler {
        fn topic(&self) -> &'static str {
            "slab.test.evt"
        }
        fn name(&self) -> &'static str {
            "panic_listener"
        }
        fn handle<'a>(
            &'a self,
            _ctx: &'a C,
            _payload: Value,
        ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
            Box::pin(async { panic!("kaboom") })
        }
    }

    #[sqlx::test]
    async fn handler_panic_is_terminal_failure_not_crash(pool: PgPool) {
        crate::pg::PgBackend::try_new(pool.clone()).await.unwrap();
        let mut registry = Registry::<()>::default();
        registry.register(PanicHandler);
        let registry = registry.freeze();

        let id = enqueue(&pool, "slab.test.evt", 1).await;

        // 订阅者 panic 被接缝处捕获：run_one_cycle 正常返回，不传播
        let processed = run_one_cycle(&pool, &(), &registry, &config())
            .await
            .unwrap();
        assert_eq!(processed, 1);

        let row = sqlx::query(
            "SELECT status, last_error FROM _pg_queue_deliveries WHERE message_id = $1 AND handler = 'panic_listener'",
        )
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
        // 3 = TerminalFailure，不重试；错误信息保留 panic 细节
        assert_eq!(row.get::<i16, _>("status"), 3);
        assert!(
            row.get::<Option<String>, _>("last_error")
                .unwrap_or_default()
                .contains("kaboom")
        );
    }

    #[sqlx::test]
    async fn failing_handler_does_not_block_others(pool: PgPool) {
        crate::pg::PgBackend::try_new(pool.clone()).await.unwrap();
        let ok = TestHandler::ok("slab.test.evt", "listener_ok");
        let bad = TestHandler::failing("slab.test.evt", "listener_bad");
        let calls_ok = ok.calls.clone();
        let calls_bad = bad.calls.clone();
        let mut registry = Registry::<()>::default();
        registry.register(ok).register(bad);
        let registry = registry.freeze();

        let id = enqueue(&pool, "slab.test.evt", 1).await; // 1 次尝试即终态

        let processed = run_one_cycle(&pool, &(), &registry, &config())
            .await
            .unwrap();
        assert_eq!(processed, 1);
        // 两个监听者都被触发
        assert_eq!(calls_ok.load(Ordering::SeqCst), 1);
        assert_eq!(calls_bad.load(Ordering::SeqCst), 1);
        // ok 成功、bad 终态失败
        assert_eq!(delivery_status(&pool, id, "listener_ok").await, (2, 0));
        assert_eq!(delivery_status(&pool, id, "listener_bad").await, (3, 1));
        // 消息行聚合为 failed（供告警）
        assert_eq!(queue_status(&pool, id).await, 3);
    }

    #[sqlx::test]
    async fn retry_is_isolated_per_handler(pool: PgPool) {
        crate::pg::PgBackend::try_new(pool.clone()).await.unwrap();
        // flaky 第一次失败（退避重试），ok 一次成功
        let ok = TestHandler::ok("slab.test.evt", "listener_ok");
        let flaky = TestHandler::flaky("slab.test.evt", "listener_flaky", 1);
        let calls_flaky = flaky.calls.clone();
        let mut registry = Registry::<()>::default();
        registry.register(ok).register(flaky);
        let registry = registry.freeze();

        let id = enqueue(&pool, "slab.test.evt", 2).await;

        // 第一轮：ok delivered，flaky 退避（pending, attempts=1）
        run_one_cycle(&pool, &(), &registry, &config())
            .await
            .unwrap();
        assert_eq!(delivery_status(&pool, id, "listener_ok").await, (2, 0));
        let (status, attempts) = delivery_status(&pool, id, "listener_flaky").await;
        assert_eq!(status, 1);
        assert_eq!(attempts, 1);
        assert_eq!(queue_status(&pool, id).await, 1); // 行保持 pending

        // 拨快 flaky 的退避时间（行与投递行一起），第二轮应只重投 flaky
        sqlx::query(
            "UPDATE _pg_queue_deliveries SET next_attempt_at = NOW() - interval '1 second' WHERE message_id = $1 AND handler = 'listener_flaky'",
        )
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "UPDATE _pg_queues SET next_attempt_at = NOW() - interval '1 second' WHERE id = $1",
        )
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();

        run_one_cycle(&pool, &(), &registry, &config())
            .await
            .unwrap();
        assert_eq!(calls_flaky.load(Ordering::SeqCst), 2);
        assert_eq!(delivery_status(&pool, id, "listener_flaky").await, (2, 1));
        assert_eq!(queue_status(&pool, id).await, 2); // 全部完成 → delivered
    }

    #[sqlx::test]
    async fn backoff_gate_respects_per_listener_timing(pool: PgPool) {
        crate::pg::PgBackend::try_new(pool.clone()).await.unwrap();
        // 两个监听者第一轮都失败退避（max_attempts=2），第二轮只拨快 A：
        // B 仍在退避中，不得被提前执行。
        let a = TestHandler::flaky("slab.test.evt", "listener_a", 1);
        let b = TestHandler::flaky("slab.test.evt", "listener_b", 1);
        let calls_a = a.calls.clone();
        let calls_b = b.calls.clone();
        let mut registry = Registry::<()>::default();
        registry.register(a).register(b);
        let registry = registry.freeze();

        let id = enqueue(&pool, "slab.test.evt", 2).await;

        // 第一轮：双方都失败 → 各自退避（pending, attempts=1）
        run_one_cycle(&pool, &(), &registry, &config())
            .await
            .unwrap();
        assert_eq!(calls_a.load(Ordering::SeqCst), 1);
        assert_eq!(calls_b.load(Ordering::SeqCst), 1);
        assert_eq!(delivery_status(&pool, id, "listener_a").await, (1, 1));
        assert_eq!(delivery_status(&pool, id, "listener_b").await, (1, 1));

        // 只拨快 A 的退避时间（消息行同步拨回，B 的投递行保持未来）
        sqlx::query(
            "UPDATE _pg_queue_deliveries SET next_attempt_at = NOW() - interval '1 second' WHERE message_id = $1 AND handler = 'listener_a'",
        )
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "UPDATE _pg_queues SET next_attempt_at = NOW() - interval '1 second' WHERE id = $1",
        )
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();

        // 第二轮：只执行 A（成功）；B 未到期不得执行
        run_one_cycle(&pool, &(), &registry, &config())
            .await
            .unwrap();
        assert_eq!(calls_a.load(Ordering::SeqCst), 2);
        assert_eq!(
            calls_b.load(Ordering::SeqCst),
            1,
            "B 仍在退避中，不得提前执行"
        );
        assert_eq!(delivery_status(&pool, id, "listener_a").await, (2, 1));
        assert_eq!(delivery_status(&pool, id, "listener_b").await, (1, 1));
    }

    #[sqlx::test]
    async fn terminal_failure_takes_precedence_in_row_state(pool: PgPool) {
        crate::pg::PgBackend::try_new(pool.clone()).await.unwrap();
        // 直接构造混合态：同一消息一个投递已终态失败、另一个 pending 到期。
        // 断言：行状态 = failed（终态失败优先于 pending，告警即时可见），
        // 但 pending 投递仍会被拉取执行，执行后行保持 failed。
        let ok = TestHandler::ok("slab.test.evt", "listener_pending");
        let calls_ok = ok.calls.clone();
        let mut registry = Registry::<()>::default();
        registry.register(ok);
        let registry = registry.freeze();

        let id = enqueue(&pool, "slab.test.evt", 2).await;
        // 手工写入两个投递行：failed 终态（attempts=2, infinity）+ pending 到期
        sqlx::query(
            r#"
            INSERT INTO _pg_queue_deliveries (message_id, handler, status, attempts, max_attempts, next_attempt_at, last_error)
            VALUES ($1, 'listener_failed', 3, 2, 2, 'infinity', 'boom'),
                   ($1, 'listener_pending', 1, 0, 2, NOW() - interval '1 second', NULL)
            "#,
        )
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "UPDATE _pg_queues SET next_attempt_at = NOW() - interval '1 second' WHERE id = $1",
        )
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();

        run_one_cycle(&pool, &(), &registry, &config())
            .await
            .unwrap();
        // pending 投递被拉取执行成功；终态失败仍在 → 行状态 = failed
        assert_eq!(calls_ok.load(Ordering::SeqCst), 1);
        assert_eq!(delivery_status(&pool, id, "listener_pending").await, (2, 0));
        assert_eq!(delivery_status(&pool, id, "listener_failed").await, (3, 2));
        assert_eq!(queue_status(&pool, id).await, 3);
    }

    #[sqlx::test]
    async fn message_without_handler_goes_terminal_failure(pool: PgPool) {
        crate::pg::PgBackend::try_new(pool.clone()).await.unwrap();
        let registry = Registry::<()>::default().freeze();

        let id = enqueue(&pool, "slab.no.subscriber", 5).await;

        run_one_cycle(&pool, &(), &registry, &config())
            .await
            .unwrap();
        assert_eq!(queue_status(&pool, id).await, 3);
        let row = sqlx::query("SELECT last_error FROM _pg_queues WHERE id = $1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert!(
            row.get::<Option<String>, _>("last_error")
                .unwrap()
                .starts_with("no_handler_for_topic:")
        );
    }

    #[sqlx::test]
    async fn late_subscriber_backfills_pending_message(pool: PgPool) {
        crate::pg::PgBackend::try_new(pool.clone()).await.unwrap();
        // 消息先入队，此时无人订阅 → 终态失败，不会投递给后到的监听者
        let id = enqueue(&pool, "slab.late.evt", 5).await;
        let registry = Registry::<()>::default().freeze();
        run_one_cycle(&pool, &(), &registry, &config())
            .await
            .unwrap();
        assert_eq!(queue_status(&pool, id).await, 3);

        // 监听者后注册，重新入队一条消息 → 正常投递
        let handler = TestHandler::ok("slab.late.evt", "listener_late");
        let calls = handler.calls.clone();
        let mut registry = Registry::<()>::default();
        registry.register(handler);
        let registry = registry.freeze();

        let id2 = enqueue(&pool, "slab.late.evt", 5).await;
        run_one_cycle(&pool, &(), &registry, &config())
            .await
            .unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(queue_status(&pool, id2).await, 2);
    }
}

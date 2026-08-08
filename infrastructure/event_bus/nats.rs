//! NATS JetStream 后端：事件直发（不经 outbox 表），延迟事件用 ADR-51 `Nats-Schedule` 头。
//!
//! - **发布**：`publish` 直接写入 JetStream stream（subject = `Event::TOPIC`），
//!   **不参与调用方 PG 事务**（与 pg 后端的同事务发布语义不同，消费端须幂等）。
//! - **延迟**：`publish_with_delay` 发布到 schedule subject，带 `Nats-Schedule: @at …` +
//!   `Nats-Schedule-Target` 头，由 JetStream 到期转投业务 subject。
//! - **消费**：每个 handler 一个 durable pull consumer（durable = `handler.name()`，filter = topic），
//!   同名 durable 多实例负载分摊；回调拿 PG 连接执行 `Subscriber::handle`（可写投影）。
//! - **可靠性**：handler 返回错误仅告警不终止；消息处理后再 ack（at-least-once，消费端幂等）。

use std::{fmt, time::Duration};

use async_nats::HeaderMap;
use async_nats::jetstream::consumer::pull;
use async_nats::jetstream::stream::{self, RetentionPolicy, StorageType};
use bytes::Bytes;
use chrono::Utc;
use futures_util::StreamExt;
use rootcause::{Result, report};
use serde::Serialize;
use tokio::sync::watch::Receiver;
use uuid::Uuid;

use crate::{FrozenRegistry, event::Event};

/// 单条消息最大投递次数（对齐 pg 后端 `max_attempts` 语义；0 表示无限）。
pub(crate) const DEFAULT_MAX_DELIVER: u64 = 5;
/// ack 等待窗口：handler 失败不 ack 后，消息在此窗口后重投。
pub(crate) const DEFAULT_ACK_WAIT: std::time::Duration = std::time::Duration::from_secs(60);

/// 可克隆的 JetStream 句柄（publish + 创建 consumer 共用同一连接）。
#[derive(Clone)]
pub struct JetStream {
    inner: async_nats::jetstream::Context,
    stream_name: String,
}

impl fmt::Debug for JetStream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("JetStream")
    }
}

impl JetStream {
    fn new(inner: async_nats::jetstream::Context, stream_name: String) -> Self {
        Self { inner, stream_name }
    }

    pub fn stream_name(&self) -> &str {
        &self.stream_name
    }

    pub fn context(&self) -> async_nats::jetstream::Context {
        self.inner.clone()
    }

    /// 序列化为 JSON 发布，等待 JetStream publish ack。
    #[tracing::instrument(skip_all)]
    pub async fn publish_json(
        &self,
        subject: impl AsRef<str>,
        event: &impl Serialize,
    ) -> Result<()> {
        let bytes = serde_json::to_vec(event)?;
        let ack = self
            .inner
            .publish(subject.as_ref().to_string(), Bytes::from(bytes))
            .await?;
        ack.await?;
        Ok(())
    }

    /// 延迟发布：ADR-51 `@at` 调度，JetStream 到期转投 `target_subject`。
    #[tracing::instrument(skip_all)]
    pub async fn publish_json_delayed(
        &self,
        target_subject: impl AsRef<str>,
        duration: Duration,
        event: &impl Serialize,
    ) -> Result<()> {
        let target_subject = target_subject.as_ref().to_string();
        let schedule_time = Utc::now() + duration;
        let schedule_subject = format!("{target_subject}.schedules.{}", Uuid::now_v7());
        let mut headers = HeaderMap::new();
        headers.append(
            "Nats-Schedule",
            format!("@at {}", schedule_time.to_rfc3339()),
        );
        headers.append("Nats-Schedule-Target", target_subject.clone());
        let bytes = serde_json::to_vec(event)?;
        let ack = self
            .inner
            .publish_with_headers(schedule_subject, headers, Bytes::from(bytes))
            .await?;
        ack.await?;
        Ok(())
    }
}

/// NATS 后端构造配置（与 CLI 对齐）。
pub struct NatsConfig {
    pub url: String,
    pub username: Option<String>,
    pub password: Option<String>,
    /// JetStream stream 名（get_or_create）。
    pub stream_name: String,
}

fn stream_subjects(stream_name: &str) -> Vec<String> {
    vec![format!("{stream_name}.>")]
}

/// 建立 NATS 连接、校验 JetStream、get_or_create Stream（File + Limits 保留，10GB）。
pub async fn connect(config: &NatsConfig) -> Result<JetStream> {
    let mut options = async_nats::ConnectOptions::new();
    if let (Some(user), Some(pass)) = (&config.username, &config.password) {
        options = options.user_and_password(user.clone(), pass.clone());
    }
    let client = options.connect(&config.url).await?;
    let info = client.server_info();
    tracing::info!(
        server_id = %info.server_id,
        version = %info.version,
        "NATS connected"
    );
    if !info.jetstream {
        return Err(report!(
            "NATS server reports JetStream disabled; start with e.g. nats-server -js"
        ));
    }
    let js = async_nats::jetstream::new(client);
    let subjects = stream_subjects(&config.stream_name);
    js.get_or_create_stream(stream::Config {
        name: config.stream_name.clone(),
        subjects,
        storage: StorageType::File,
        retention: RetentionPolicy::Limits,
        max_bytes: 10 * 1024 * 1024 * 1024, // 10GB
        allow_message_schedules: true,
        allow_message_ttl: true,
        ..Default::default()
    })
    .await?;
    tracing::info!(stream = %config.stream_name, "JetStream stream ready");
    Ok(JetStream::new(js, config.stream_name.clone()))
}

/// NATS 后端：JetStream + PG 池（handler 回调用）。
#[derive(Clone)]
pub struct NatsBackend {
    js: JetStream,
}

impl NatsBackend {
    pub async fn try_new(config: NatsConfig) -> Result<Self> {
        let js = connect(&config).await?;
        Ok(Self { js })
    }
}

impl NatsBackend {
    pub(crate) async fn publish<T: Event>(&self, event: &T) -> Result<()> {
        self.js.publish_json(T::TOPIC, event).await
    }

    pub(crate) async fn publish_delayed<T: Event>(&self, event: &T, delay: Duration) -> Result<()> {
        self.js.publish_json_delayed(T::TOPIC, delay, event).await
    }
}

/// 每个 handler 一个 durable pull consumer；`durable` 同名多实例负载分摊。
/// `ctx`（如 `AppCtx`）克隆进每个 consumer task，原样传给 `Subscriber::handle`。
pub(crate) async fn run_nats_dispatcher<C: Send + Sync + Clone + 'static>(
    backend: &NatsBackend,
    ctx: C,
    registry: FrozenRegistry<C>,
    shutdown: Receiver<bool>,
    ack_wait: Duration,
) -> Result<()> {
    let js_ctx = backend.js.context();
    let stream = backend.js.stream_name().to_string();

    // 为每个已注册 handler 创建 durable consumer（广播语义：同一 topic 多个 handler 各自游标）。
    let mut tasks = Vec::new();
    for (topic, handlers) in registry.iter() {
        for handler in handlers {
            let durable = handler.name().to_string();
            let filter_subject = (*topic).to_string();
            let consumer = js_ctx
                .create_consumer_on_stream(
                    pull::Config {
                        durable_name: Some(durable.clone()),
                        filter_subject,
                        // 有限重投（对齐 pg 后端的 max_attempts）：handler 失败不 ack，
                        // ack_wait 到期后 redeliver，超过 max_deliver 后丢弃（日志告警）。
                        max_deliver: DEFAULT_MAX_DELIVER as i64,
                        ack_wait,
                        ..Default::default()
                    },
                    stream.as_str(),
                )
                .await?;
            let mut messages = consumer.messages().await?;
            let handler = handler.clone();
            let ctx = ctx.clone();
            let mut shutdown = shutdown.clone();
            tasks.push(tokio::spawn(async move {
                loop {
                    tokio::select! {
                        _ = shutdown.changed() => {
                            if *shutdown.borrow() {
                                tracing::info!(durable, "JetStream pull consumer shutdown");
                                break;
                            }
                        }
                        next = messages.next() => {
                            match next {
                                None => break,
                                Some(Ok(msg)) => {
                                    let Ok(payload) = serde_json::from_slice(&msg.payload) else {
                                        tracing::warn!(durable, "JetStream invalid payload");
                                        continue;
                                    };
                                    if handler.handle(&ctx, payload).await.is_ok() {
                                        if let Err(e) = msg.ack().await {
                                            tracing::warn!(durable, error = %e, "ack failed");
                                        }
                                    } else {
                                        // 不 ack：ack_wait 到期后 redeliver（at-least-once），超 max_deliver 丢弃。
                                        tracing::warn!(durable, "handler failed, message will be redelivered");
                                    }
                                }
                                Some(Err(e)) => {
                                    tracing::error!(durable, error = %e, "JetStream stream error");
                                }
                            }
                        }
                    }
                }
            }));
        }
    }

    for task in tasks {
        task.await?;
    }
    Ok(())
}

#[cfg(all(test, feature = "nats"))]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};
    use shared_contract::event::Event;

    #[derive(Debug, Serialize, Deserialize)]
    struct TestEvent {
        n: i32,
    }
    impl Event for TestEvent {
        const TOPIC: &'static str = "slab.nats_test.evt";
    }

    /// 需要本地 NATS（默认跳过）：`NATS_TEST_URL=... cargo test -p queue --features nats -- --ignored`
    fn test_url() -> Option<String> {
        std::env::var("NATS_TEST_URL").ok()
    }

    #[tokio::test]
    #[ignore]
    async fn publish_roundtrip() {
        let Some(url) = test_url() else {
            eprintln!("NATS_TEST_URL not set, skipping nats backend test");
            return;
        };
        let config = NatsConfig {
            url,
            username: None,
            password: None,
            stream_name: "slab".to_string(),
        };
        let js = connect(&config).await.unwrap();
        js.publish_json(TestEvent::TOPIC, &TestEvent { n: 7 })
            .await
            .unwrap();
    }

    #[tokio::test]
    #[ignore]
    async fn publish_delayed_roundtrip() {
        let Some(url) = test_url() else {
            eprintln!("NATS_TEST_URL not set, skipping nats backend test");
            return;
        };
        let config = NatsConfig {
            url,
            username: None,
            password: None,
            stream_name: "slab".to_string(),
        };
        let js = connect(&config).await.unwrap();
        js.publish_json_delayed(
            TestEvent::TOPIC,
            Duration::from_secs(1),
            &TestEvent { n: 7 },
        )
        .await
        .unwrap();
    }
}

#[cfg(all(test, feature = "nats"))]
mod e2e_tests {
    use super::*;
    use crate::Subscriber;
    use crate::registry::Registry;
    use serde::{Deserialize, Serialize};
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::sync::watch;

    /// 唯一 durable 名：每次运行全新 consumer（JetStream 对已存在 durable 复用旧 filter 配置）。
    fn unique_durable(prefix: &str) -> &'static str {
        static SEQ: AtomicUsize = AtomicUsize::new(0);
        Box::leak(format!("{prefix}_{}", SEQ.fetch_add(1, Ordering::SeqCst)).into_boxed_str())
    }

    macro_rules! e2e_event {
        ($name:ident, $topic:literal) => {
            #[derive(Debug, Serialize, Deserialize)]
            struct $name {
                n: i32,
            }
            impl Event for $name {
                const TOPIC: &'static str = $topic;
            }
        };
    }
    // 每个测试独立 topic：避免共享 stream 下多 durable 竞争（可并行运行）。
    e2e_event!(ConEvent, "slab.e2e.con.evt");
    e2e_event!(FlakyEvent, "slab.e2e.flaky.evt");
    e2e_event!(DelayEvent, "slab.e2e.delay.evt");
    e2e_event!(BroadcastEvent, "slab.e2e.bc.evt");

    struct CountHandler {
        topic: &'static str,
        name: &'static str,
        calls: Arc<AtomicUsize>,
    }
    impl<C: Send + Sync + 'static> Subscriber<C> for CountHandler {
        fn topic(&self) -> &'static str {
            self.topic
        }
        fn name(&self) -> &'static str {
            self.name
        }
        fn handle<'a>(
            &'a self,
            _ctx: &'a C,
            _payload: serde_json::Value,
        ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
            let calls = self.calls.clone();
            Box::pin(async move {
                calls.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
        }
    }

    fn test_url() -> Option<String> {
        std::env::var("NATS_TEST_URL").ok()
    }

    /// 端到端：publish → durable consumer 消费（真实 nats-server，需 `-js`）。
    /// `NATS_TEST_URL=... cargo test -p queue --features nats -- --ignored`
    #[tokio::test]
    #[ignore]
    async fn publish_consume_roundtrip() {
        let Some(url) = test_url() else {
            eprintln!("NATS_TEST_URL not set, skipping nats e2e test");
            return;
        };
        let config = NatsConfig {
            url,
            username: None,
            password: None,
            stream_name: "slab".to_string(),
        };
        let backend = NatsBackend::try_new(config).await.unwrap();

        // 先注册 handler（durable consumer 幂等，重复跑不冲突）。
        let calls = Arc::new(AtomicUsize::new(0));
        let mut registry = Registry::<()>::default();
        registry.register(CountHandler {
            topic: ConEvent::TOPIC,
            name: unique_durable("e2e_count"),
            calls: calls.clone(),
        });
        let registry = registry.freeze();

        // 发布 2 条 → 启动消费循环 → 等待 handler 计数到达。
        backend.publish(&ConEvent { n: 1 }).await.unwrap();
        backend.publish(&ConEvent { n: 2 }).await.unwrap();

        let (tx, rx) = watch::channel(false);
        // Box::leak：dispatcher task 需要 'static 借用（测试进程内泄漏无害）。
        let backend = Box::leak(Box::new(backend));
        let dispatcher = tokio::spawn(run_nats_dispatcher(
            backend,
            (),
            registry,
            rx,
            Duration::from_secs(60),
        ));

        let ok = tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                if calls.load(Ordering::SeqCst) >= 2 {
                    return true;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await
        .unwrap_or(false);
        assert!(ok, "handler should consume both messages");

        let _ = tx.send(true);
        let _ = tokio::time::timeout(Duration::from_secs(5), dispatcher).await;
    }

    /// handler 前 N 次失败：不 ack → ack_wait 后 redeliver → 最终成功。
    /// 验证 at-least-once + 有限重投语义（ack_wait 缩短为 1s 以加速测试）。
    #[tokio::test]
    #[ignore]
    async fn handler_failure_redelivers_then_succeeds() {
        let Some(url) = test_url() else {
            eprintln!("NATS_TEST_URL not set, skipping nats e2e test");
            return;
        };
        let config = NatsConfig {
            url,
            username: None,
            password: None,
            stream_name: "slab".to_string(),
        };
        let backend = Box::leak(Box::new(NatsBackend::try_new(config).await.unwrap()));

        struct Flaky {
            attempts: Arc<AtomicUsize>,
            calls: Arc<AtomicUsize>,
        }
        impl<C: Send + Sync + 'static> Subscriber<C> for Flaky {
            fn topic(&self) -> &'static str {
                FlakyEvent::TOPIC
            }
            fn name(&self) -> &'static str {
                unique_durable("e2e_flaky")
            }
            fn handle<'a>(
                &'a self,
                _ctx: &'a C,
                _payload: serde_json::Value,
            ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
                let attempts = self.attempts.clone();
                let calls = self.calls.clone();
                Box::pin(async move {
                    let n = attempts.fetch_add(1, Ordering::SeqCst);
                    calls.fetch_add(1, Ordering::SeqCst);
                    if n < 2 {
                        Err(report!("transient failure"))
                    } else {
                        Ok(())
                    }
                })
            }
        }

        let attempts = Arc::new(AtomicUsize::new(0));
        let calls = Arc::new(AtomicUsize::new(0));
        let mut registry = Registry::<()>::default();
        registry.register(Flaky {
            attempts: attempts.clone(),
            calls: calls.clone(),
        });
        let registry = registry.freeze();

        backend.publish(&FlakyEvent { n: 9 }).await.unwrap();

        let (tx, rx) = watch::channel(false);
        let dispatcher = tokio::spawn(run_nats_dispatcher(
            backend,
            (),
            registry,
            rx,
            Duration::from_secs(1), // 短 ack_wait：失败后 1s 重投
        ));

        let ok = tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                // 前 2 次失败 + 第 3 次成功 = 3 次调用
                if calls.load(Ordering::SeqCst) >= 3 {
                    return true;
                }
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
        })
        .await
        .unwrap_or(false);
        assert!(
            ok,
            "flaky handler should be retried until success (3 calls)"
        );

        let _ = tx.send(true);
        let _ = tokio::time::timeout(Duration::from_secs(5), dispatcher).await;
    }

    /// 延迟发布（ADR-51 schedule）：到期转投后 durable consumer 收到并消费。
    #[tokio::test]
    #[ignore]
    async fn delayed_event_is_delivered_after_schedule() {
        let Some(url) = test_url() else {
            eprintln!("NATS_TEST_URL not set, skipping nats e2e test");
            return;
        };
        let config = NatsConfig {
            url,
            username: None,
            password: None,
            stream_name: "slab".to_string(),
        };
        let backend = Box::leak(Box::new(NatsBackend::try_new(config).await.unwrap()));

        let calls = Arc::new(AtomicUsize::new(0));
        let mut registry = Registry::<()>::default();
        registry.register(CountHandler {
            topic: ConEvent::TOPIC,
            name: unique_durable("e2e_count"),
            calls: calls.clone(),
        });
        let registry = registry.freeze();

        // 1 秒后转投
        backend
            .publish_delayed(&DelayEvent { n: 42 }, Duration::from_secs(1))
            .await
            .unwrap();

        let (tx, rx) = watch::channel(false);
        let dispatcher = tokio::spawn(run_nats_dispatcher(
            backend,
            (),
            registry,
            rx,
            Duration::from_secs(60),
        ));

        // 应在 ~1s 后（转投）被消费，而非立即
        let ok = tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                if calls.load(Ordering::SeqCst) >= 1 {
                    return true;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await
        .unwrap_or(false);
        assert!(ok, "delayed event should be delivered after schedule");

        let _ = tx.send(true);
        let _ = tokio::time::timeout(Duration::from_secs(5), dispatcher).await;
    }

    /// 广播：同一 topic 两个 handler，各自 durable consumer 独立消费同一条消息。
    #[tokio::test]
    #[ignore]
    async fn broadcast_delivers_to_every_handler() {
        let Some(url) = test_url() else {
            eprintln!("NATS_TEST_URL not set, skipping nats e2e test");
            return;
        };
        let config = NatsConfig {
            url,
            username: None,
            password: None,
            stream_name: "slab".to_string(),
        };
        let backend = Box::leak(Box::new(NatsBackend::try_new(config).await.unwrap()));

        let calls_a = Arc::new(AtomicUsize::new(0));
        let calls_b = Arc::new(AtomicUsize::new(0));
        let mut registry = Registry::<()>::default();
        registry
            .register(CountHandler {
                topic: BroadcastEvent::TOPIC,
                name: unique_durable("e2e_broadcast_a"),
                calls: calls_a.clone(),
            })
            .register(CountHandler {
                topic: BroadcastEvent::TOPIC,
                name: unique_durable("e2e_broadcast_b"),
                calls: calls_b.clone(),
            });
        let registry = registry.freeze();

        backend.publish(&BroadcastEvent { n: 7 }).await.unwrap();

        let (tx, rx) = watch::channel(false);
        let dispatcher = tokio::spawn(run_nats_dispatcher(
            backend,
            (),
            registry,
            rx,
            Duration::from_secs(60),
        ));

        let ok = tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                if calls_a.load(Ordering::SeqCst) >= 1 && calls_b.load(Ordering::SeqCst) >= 1 {
                    return true;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await
        .unwrap_or(false);
        assert!(ok, "both handlers should consume the same message");

        let _ = tx.send(true);
        let _ = tokio::time::timeout(Duration::from_secs(5), dispatcher).await;
    }
}

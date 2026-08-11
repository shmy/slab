//! Metrics 埋点：请求延迟 / sqlx 查询耗时 / 队列与事件总线积压。
//!
//! 仪表经 `opentelemetry::global::meter("slab")` 取用（宏生成，见下）；未注册 meter provider
//! 时 global meter 为 noop，record 零开销（测试环境安全）。
//!
//! - 请求延迟：`record_request_metrics` axum 中间件，直方图 `http.server.request.duration`
//!   （OTel 语义约定命名），属性 method / route（MatchedPath 模板）/ status。
//! - sqlx 耗时：`SqlxQueryMetricsLayer` tracing Layer（裸 Layer + 自过滤，见 struct doc），
//!   消费 `sqlx::query` 日志事件的 `elapsed_secs` 字段 → 直方图 `db.query.duration`；
//!   日志层由 EnvFilter 的 `sqlx::query=off` 屏蔽同一事件的噪音输出。
//! - 积压：`internal_jobs.rs` 的 `BacklogMetrics` 周期任务采样（调 `JobBus::backlog` / `EventBus::backlog`）。

use std::sync::OnceLock;

use axum::extract::{MatchedPath, Request};
use axum::middleware::Next;
use axum::response::Response;
use opentelemetry::KeyValue;
use opentelemetry::global::meter;
use opentelemetry::metrics::{Gauge, Histogram};

use tracing::field::{Field, Visit};
use tracing::{Event, Metadata, Subscriber};
use tracing_subscriber::layer::Context;

const METER_NAME: &str = "slab";

/// 直方图仪表（秒）：OnceLock 缓存句柄（每次 `meter.*(...).build()` 都有 registry 查找，
/// 热点路径不该重复构造）。首次调用时经 global meter 创建——必须在
/// `trace_kit::init_tracing`（注册 meter provider）之后首次使用。
macro_rules! histogram_instrument {
    ($($fn:ident, $name:literal, $desc:literal);+ $(;)?) => {
        $(
            pub(crate) fn $fn() -> &'static Histogram<f64> {
                static H: OnceLock<Histogram<f64>> = OnceLock::new();
                H.get_or_init(|| {
                    meter(METER_NAME)
                        .f64_histogram($name)
                        .with_unit("s")
                        .with_description($desc)
                        .build()
                })
            }
        )+
    };
}

/// gauge 仪表（i64）：同上。
macro_rules! gauge_instrument {
    ($($fn:ident, $name:literal, $desc:literal);+ $(;)?) => {
        $(
            pub(crate) fn $fn() -> &'static Gauge<i64> {
                static H: OnceLock<Gauge<i64>> = OnceLock::new();
                H.get_or_init(|| {
                    meter(METER_NAME)
                        .i64_gauge($name)
                        .with_description($desc)
                        .build()
                })
            }
        )+
    };
}

// 耗时直方图（秒）。
histogram_instrument! {
    http_request_duration, "http.server.request.duration", "HTTP server request duration";
    db_query_duration, "db.query.duration", "SQL query execution duration";
}

// 连接池 USE（官方 `Pool::size` / `num_idle`，对应 sqlx issue #1896 诉求——官方 API 直接采样）。
gauge_instrument! {
    db_pool_connections, "db.pool.connections", "current connections in the pool (sqlx Pool::size)";
    db_pool_idle, "db.pool.idle", "idle connections in the pool (sqlx Pool::num_idle)";
}

// 队列积压（worker_jobs 按状态计数）。
gauge_instrument! {
    job_pending, "job_queue.pending", "worker_jobs rows in Pending state";
    job_running, "job_queue.running", "worker_jobs rows in Running state";
    job_failed, "job_queue.failed", "worker_jobs rows in terminal Failed state";
}

// 事件总线积压（_pg_events / _pg_event_deliveries）。
gauge_instrument! {
    event_pending, "event_bus.pending", "_pg_events rows pending delivery";
    event_failed, "event_bus.failed", "_pg_events rows in failed state";
    event_deliveries_pending, "event_bus.deliveries.pending", "_pg_event_deliveries rows pending delivery";
}

/// sqlx 查询事件 → 耗时直方图的 tracing Layer。
///
/// 消费 `sqlx::query` 日志事件的 `elapsed_secs` 字段（db 池开 `log_statements(Info)` 产生）；
/// `on_event` 校验 target 后记录，其余事件原样放行。
pub(crate) struct SqlxQueryMetricsLayer;

impl<S: Subscriber> tracing_subscriber::layer::Layer<S> for SqlxQueryMetricsLayer {
    // enabled / event_enabled 恒 true：`Layered::{enabled,event_enabled}` 是 AND 链，
    // 任一层返回 false 会全局丢弃事件（per-layer 语义在 on_event 里实现，与
    // `Filtered::event_enabled` 拒绝时返回 true 同构）。
    fn enabled(&self, _metadata: &Metadata<'_>, _ctx: Context<'_, S>) -> bool {
        true
    }

    fn event_enabled(&self, _event: &Event<'_>, _ctx: Context<'_, S>) -> bool {
        true
    }

    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        if event.metadata().target() != "sqlx::query" {
            return;
        }
        let mut elapsed = ElapsedSecs(0.0);
        event.record(&mut elapsed);
        if elapsed.0 > 0.0 {
            db_query_duration().record(elapsed.0, &[KeyValue::new("db.system", "postgresql")]);
        }
    }
}

/// 从事件字段中取 `elapsed_secs`（f64）的 Visit。
struct ElapsedSecs(f64);

impl Visit for ElapsedSecs {
    fn record_f64(&mut self, field: &Field, value: f64) {
        if field.name() == "elapsed_secs" {
            self.0 = value;
        }
    }
    // 其余字段（summary / db.statement / rows_* / elapsed 等）不采集。
    fn record_debug(&mut self, _field: &Field, _value: &dyn std::fmt::Debug) {}
}

/// 请求延迟中间件：记录 method / 路由模板（MatchedPath）/ 状态码 → `http.server.request.duration`。
pub async fn record_request_metrics(req: Request, next: Next) -> Response {
    let start = std::time::Instant::now();
    let method = req.method().as_str().to_owned();
    let route = req
        .extensions()
        .get::<MatchedPath>()
        .map(|m| m.as_str().to_owned())
        .unwrap_or_default();
    let response = next.run(req).await;
    let status = i64::from(response.status().as_u16());
    http_request_duration().record(
        start.elapsed().as_secs_f64(),
        &[
            KeyValue::new("http.request.method", method),
            KeyValue::new("http.route", route),
            KeyValue::new("http.response.status_code", status),
        ],
    );
    response
}

#[cfg(test)]
mod tests {
    use crate::metrics::SqlxQueryMetricsLayer;
    use opentelemetry_sdk::metrics::data::{AggregatedMetrics, Metric, MetricData};
    use opentelemetry_sdk::metrics::{InMemoryMetricExporter, PeriodicReader, SdkMeterProvider};
    use tracing_subscriber::layer::{Layer as _, SubscriberExt as _};

    /// 串行验证（避免 set_meter_provider 全局替换 + OnceLock instrument 绑定的测试污染）：
    /// 1. 裸 Layer 自过滤：sqlx::query 事件 → db.query.duration 直方图，EnvFilter=off 不影响；
    /// 2. 生产组装复刻：Box<dyn Layer> + and_then reduce + 多层叠 + global dispatch 同样收到。
    #[test]
    fn sqlx_query_events_flow_through_metrics_layer() {
        let exporter = InMemoryMetricExporter::default();
        let provider = SdkMeterProvider::builder()
            .with_reader(PeriodicReader::builder(exporter.clone()).build())
            .build();
        opentelemetry::global::set_meter_provider(provider.clone());

        // 场景 1：裸 Layer + EnvFilter(sqlx::query=off) 的日志层（per-layer 语义互不干扰）。
        let subscriber = tracing_subscriber::registry()
            .with(SqlxQueryMetricsLayer)
            .with(
                tracing_subscriber::fmt::layer()
                    .with_filter(tracing_subscriber::EnvFilter::new("off,sqlx::query=off")),
            );
        let guard = tracing::subscriber::set_default(subscriber);

        tracing::info!(target: "sqlx::query", summary = "SELECT 1", elapsed_secs = 0.042);
        tracing::info!("ordinary log");
        drop(guard);

        // 场景 2：生产组装（main.rs 同款）：Box + reduce + 多层 + global dispatch。
        let extra: Vec<
            Box<
                dyn tracing_subscriber::layer::Layer<tracing_subscriber::registry::Registry>
                    + Send
                    + Sync,
            >,
        > = vec![Box::new(SqlxQueryMetricsLayer)];
        let stack = extra
            .into_iter()
            .reduce(|acc, layer| Box::new(acc.and_then(layer)))
            .expect("non-empty");
        let filter = tracing_subscriber::EnvFilter::new("debug");
        let subscriber = tracing_subscriber::registry()
            .with(stack)
            .with(tracing_subscriber::fmt::layer().with_filter(filter.clone()))
            .with(tracing_subscriber::fmt::layer().with_filter(filter.clone()))
            .with(tracing_subscriber::fmt::layer().with_filter(filter.clone()));
        let _ = tracing::subscriber::set_global_default(subscriber);

        tracing::info!(target: "sqlx::query", summary = "SELECT 2", elapsed_secs = 0.017);

        provider.force_flush().expect("flush metrics");
        let finished = exporter.get_finished_metrics().expect("finished metrics");
        let metric: Option<&Metric> = finished
            .iter()
            .flat_map(|rm| rm.scope_metrics())
            .flat_map(|scope| scope.metrics())
            .find(|m| m.name() == "db.query.duration");
        let metric = metric.expect("db.query.duration must be recorded in both scenarios");
        let AggregatedMetrics::F64(MetricData::Histogram(hist)) = metric.data() else {
            panic!("expected f64 histogram for db.query.duration");
        };
        // 同一 instrument 无属性 → 两场景数据点累积为同一时间序列（count=2, sum=0.059）。
        // 注意：若未来给直方图加属性/换 instrument，此断言需同步拆成两段验证。
        let dps: Vec<_> = hist.data_points().collect();
        assert_eq!(
            dps.len(),
            1,
            "both scenarios aggregate into one time series"
        );
        assert_eq!(dps[0].count(), 2, "both scenarios must contribute");
        assert!(
            (dps[0].sum() - 0.059).abs() < 1e-9,
            "sum = {:?}",
            dps[0].sum()
        );
    }
}

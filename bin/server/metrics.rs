//! Metrics 埋点：请求延迟 / sqlx 查询耗时 / 队列与事件总线积压。
//!
//! 仪表经 `opentelemetry::global::meter("slab")` 取用，`OnceLock` 缓存句柄
//! （每次 `meter.*(...).build()` 都有 registry 查找，热点路径不该重复构造）；
//! 未配置 OTLP（测试/本地无 `set_meter_provider`）时 global meter 为 noop，record 零开销。
//!
//! - 请求延迟：`record_request_metrics` axum 中间件，直方图 `http.server.request.duration`
//!   （OTel 语义约定命名），属性 method / route（MatchedPath 模板）/ status。
//! - sqlx 耗时：`SqlxQueryMetricsLayer` tracing Layer，消费 `sqlx::query` 日志事件的
//!   `elapsed_secs` 字段 → 直方图 `db.query.duration`；注册在 EnvFilter 之前（自带 Targets
//!   过滤，见 trace_kit::init_tracing 注释），日志层由 `sqlx::query=off` 屏蔽噪音。
//! - 积压：`internal_jobs.rs` 的 `BacklogMetrics` 周期任务采样（调 `JobBus::backlog` / `EventBus::backlog`）。

use std::sync::OnceLock;

use axum::extract::{MatchedPath, Request};
use axum::middleware::Next;
use axum::response::Response;
use opentelemetry::KeyValue;
use opentelemetry::global::meter;
use opentelemetry::metrics::{Gauge, Histogram};
use tracing::Level;
use tracing::field::{Field, Visit};
use tracing::{Event, Subscriber};
use tracing_subscriber::filter::{Filtered, Targets};
use tracing_subscriber::layer::{Context, Layer as _};
use tracing_subscriber::registry::Registry;

const METER_NAME: &str = "slab";

/// HTTP 请求延迟直方图（秒）。
pub fn http_request_duration() -> &'static Histogram<f64> {
    static H: OnceLock<Histogram<f64>> = OnceLock::new();
    H.get_or_init(|| {
        meter(METER_NAME)
            .f64_histogram("http.server.request.duration")
            .with_unit("s")
            .with_description("HTTP server request duration")
            .build()
    })
}

/// sqlx 查询耗时直方图（秒）。
pub fn db_query_duration() -> &'static Histogram<f64> {
    static H: OnceLock<Histogram<f64>> = OnceLock::new();
    H.get_or_init(|| {
        meter(METER_NAME)
            .f64_histogram("db.query.duration")
            .with_unit("s")
            .with_description("SQL query execution duration")
            .build()
    })
}

/// 连接池当前连接数 gauge（官方 `Pool::size`；与 `num_idle` 一起可算利用率/饱和度，
/// 对应 sqlx issue #1896 的 USE 指标诉求——官方 API 直接采样，无需等待官方落地）。
pub fn db_pool_connections() -> &'static Gauge<i64> {
    static H: OnceLock<Gauge<i64>> = OnceLock::new();
    H.get_or_init(|| {
        meter(METER_NAME)
            .i64_gauge("db.pool.connections")
            .with_description("current connections in the pool (sqlx Pool::size)")
            .build()
    })
}

/// 连接池空闲连接数 gauge（官方 `Pool::num_idle`）。
pub fn db_pool_idle() -> &'static Gauge<i64> {
    static H: OnceLock<Gauge<i64>> = OnceLock::new();
    H.get_or_init(|| {
        meter(METER_NAME)
            .i64_gauge("db.pool.idle")
            .with_description("idle connections in the pool (sqlx Pool::num_idle)")
            .build()
    })
}

/// 队列积压 gauge（worker_jobs 按状态计数）。
pub fn job_pending() -> &'static Gauge<i64> {
    static H: OnceLock<Gauge<i64>> = OnceLock::new();
    H.get_or_init(|| {
        meter(METER_NAME)
            .i64_gauge("job_queue.pending")
            .with_description("worker_jobs rows in Pending state")
            .build()
    })
}

pub fn job_running() -> &'static Gauge<i64> {
    static H: OnceLock<Gauge<i64>> = OnceLock::new();
    H.get_or_init(|| {
        meter(METER_NAME)
            .i64_gauge("job_queue.running")
            .with_description("worker_jobs rows in Running state")
            .build()
    })
}

pub fn job_failed() -> &'static Gauge<i64> {
    static H: OnceLock<Gauge<i64>> = OnceLock::new();
    H.get_or_init(|| {
        meter(METER_NAME)
            .i64_gauge("job_queue.failed")
            .with_description("worker_jobs rows in terminal Failed state")
            .build()
    })
}

/// 事件总线积压 gauge（_pg_events 按状态计数）。
pub fn event_pending() -> &'static Gauge<i64> {
    static H: OnceLock<Gauge<i64>> = OnceLock::new();
    H.get_or_init(|| {
        meter(METER_NAME)
            .i64_gauge("event_bus.pending")
            .with_description("_pg_events rows pending delivery")
            .build()
    })
}

pub fn event_failed() -> &'static Gauge<i64> {
    static H: OnceLock<Gauge<i64>> = OnceLock::new();
    H.get_or_init(|| {
        meter(METER_NAME)
            .i64_gauge("event_bus.failed")
            .with_description("_pg_events rows in failed state")
            .build()
    })
}

/// 事件投递积压 gauge（_pg_event_deliveries 待投递 handler 数）。
pub fn event_deliveries_pending() -> &'static Gauge<i64> {
    static H: OnceLock<Gauge<i64>> = OnceLock::new();
    H.get_or_init(|| {
        meter(METER_NAME)
            .i64_gauge("event_bus.deliveries.pending")
            .with_description("_pg_event_deliveries rows pending delivery")
            .build()
    })
}

/// sqlx 查询事件 → 耗时直方图的 tracing Layer。
///
/// 只消费 target `sqlx::query` 的事件；自带 Targets 过滤（注册于 EnvFilter 之前，
/// 不被日志级别掐断），日志层由 EnvFilter 的 `sqlx::query=off` 指令屏蔽同一事件的噪音输出。
pub(crate) struct SqlxQueryMetricsLayer;

impl<S: Subscriber> tracing_subscriber::layer::Layer<S> for SqlxQueryMetricsLayer {
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

/// 构造 sqlx 指标层：`Filtered` 自带 per-layer Targets 过滤，只放行 `sqlx::query` @ INFO。
pub(crate) fn sqlx_query_layer() -> Filtered<SqlxQueryMetricsLayer, Targets, Registry> {
    SqlxQueryMetricsLayer.with_filter(Targets::new().with_target("sqlx::query", Level::INFO))
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
    use super::*;
    use opentelemetry::metrics::MeterProvider as _;
    use opentelemetry_sdk::metrics::data::{AggregatedMetrics, Metric, MetricData};
    use opentelemetry_sdk::metrics::{InMemoryMetricExporter, PeriodicReader, SdkMeterProvider};
    use tracing_subscriber::filter::Targets;
    use tracing_subscriber::layer::SubscriberExt as _;

    /// 端到端验证：sqlx::query 日志事件（含 elapsed_secs 字段）→ `db.query.duration` 直方图。
    /// 同时验证 EnvFilter 层的 `sqlx::query=off` 不掐断事件（per-layer Filter 语义，
    /// Filtered::enabled 恒 true，由 Registry FilterState 按层裁决）。
    #[test]
    fn sqlx_query_event_records_duration_histogram() {
        // 1. 内存 exporter + 全局 meter provider（global 只能设一次，本测试进程内唯一）。
        let exporter = InMemoryMetricExporter::default();
        let provider = SdkMeterProvider::builder()
            .with_reader(PeriodicReader::builder(exporter.clone()).build())
            .build();
        opentelemetry::global::set_meter_provider(provider.clone());

        // 2. 模拟 init_tracing 的组合：sqlx 指标层（独立 Targets）+ EnvFilter 过滤的日志层。
        let subscriber = tracing_subscriber::registry()
            .with(
                SqlxQueryMetricsLayer
                    .with_filter(Targets::new().with_target("sqlx::query", Level::INFO)),
            )
            .with(
                tracing_subscriber::fmt::layer()
                    .with_filter(tracing_subscriber::EnvFilter::new("off,sqlx::query=off")),
            );
        let _guard = tracing::subscriber::set_default(subscriber);

        // 3. 模拟 sqlx QueryLogger 的完成事件（target + elapsed_secs 字段）。
        tracing::info!(target: "sqlx::query", summary = "SELECT 1", elapsed_secs = 0.042);
        // 非 sqlx 事件不应被采集。
        tracing::info!("ordinary log");

        // 4. 冲刷并断言。
        provider.force_flush().expect("flush metrics");
        let finished = exporter
            .get_finished_metrics()
            .expect("get finished metrics");
        let metric: Option<&Metric> = finished
            .iter()
            .flat_map(|rm| rm.scope_metrics())
            .flat_map(|scope| scope.metrics())
            .find(|m| m.name() == "db.query.duration");
        let metric = metric.expect("db.query.duration must be recorded");
        let AggregatedMetrics::F64(MetricData::Histogram(hist)) = metric.data() else {
            panic!("expected f64 histogram for db.query.duration");
        };
        let mut points = hist.data_points();
        let dp = points.next().expect("one histogram data point");
        assert!(points.next().is_none(), "only one data point expected");
        assert_eq!(dp.count(), 1);
        assert!((dp.sum() - 0.042).abs() < 1e-9);
    }
}

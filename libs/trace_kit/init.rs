#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "init-time panics are acceptable; missing OTLP endpoint is a deployment error"
)]
#[cfg(feature = "otlp")]
use opentelemetry::{KeyValue, StringValue, Value};
#[cfg(feature = "otlp")]
use opentelemetry_sdk::Resource;
#[cfg(feature = "otlp")]
use opentelemetry_semantic_conventions::{SCHEMA_URL, resource::SERVICE_VERSION};
use rootcause::hooks::Hooks;
use rootcause_tracing::{RootcauseLayer, SpanCollector};
use tracing_subscriber::{
    EnvFilter, layer::Layer, layer::SubscriberExt as _, registry::Registry,
    util::SubscriberInitExt as _,
};

/// 日志过滤（各层按需取用）：默认 info，OTel 桥接链路全程 trace；
/// `sqlx::query=off` 屏蔽 sqlx 逐条 SQL 日志噪音（其 `elapsed_secs` 由指标层单独消费）；
/// `opentelemetry/hyper/tonic/reqwest=off` 防 telemetry-induced-telemetry（导出栈自身日志
/// 不再回灌 OTLP）。显式设置 RUST_LOG 时以用户为准（不强制追加）。
fn log_filter(config: &TraceConfig) -> EnvFilter {
    EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new(format!(
            "{},otel::tracing=trace,sqlx::query=off,opentelemetry=off,hyper=off,tonic=off,reqwest=off",
            config.level
        ))
    })
}

/// OTLP/gRPC 传输配置（OpenObserve 官方 Rust 指南）：
/// - endpoint：`{otlp_endpoint}`（base URL；gRPC 的 org 不进 URL 路径）；
/// - metadata：OTLP_METADATA JSON 原样转 gRPC metadata，并**强制补 organization**——
///   OpenObserve 的 gRPC 路径要求 `organization` header 与 `Authorization` 同传。
#[cfg(feature = "otlp")]
fn otlp_grpc_config(
    endpoint: &str,
    metadata: &str,
) -> (
    String,
    opentelemetry_otlp::tonic_types::metadata::MetadataMap,
) {
    use std::str::FromStr as _;
    let mut map: std::collections::HashMap<String, String> =
        serde_json::from_str(metadata).expect("parse otlp_metadata");
    if !map.contains_key("organization") {
        map.insert("organization".to_string(), "default".to_string());
    }
    let mut m = opentelemetry_otlp::tonic_types::metadata::MetadataMap::new();
    for (k, v) in map {
        let key = tonic::metadata::MetadataKey::from_str(&k).expect("metadata key");
        let value = tonic::metadata::MetadataValue::from_str(&v).expect("metadata value");
        m.insert(key, value);
    }
    (endpoint.to_string(), m)
}

#[derive(Debug)]
pub struct TraceConfig<'a> {
    level: &'a str,
    #[cfg(feature = "otlp")]
    otlp_service_name: &'a str,
    #[cfg(feature = "otlp")]
    otlp_endpoint: &'a str,
    #[cfg(feature = "otlp")]
    otlp_metadata: &'a str,
}

impl<'a> TraceConfig<'a> {
    pub fn new(
        level: &'a str,
        #[cfg(feature = "otlp")] otlp_service_name: &'a str,
        #[cfg(feature = "otlp")] otlp_endpoint: &'a str,
        #[cfg(feature = "otlp")] otlp_metadata: &'a str,
    ) -> Self {
        Self {
            level,
            #[cfg(feature = "otlp")]
            otlp_service_name,
            #[cfg(feature = "otlp")]
            otlp_endpoint,
            #[cfg(feature = "otlp")]
            otlp_metadata,
        }
    }
}

pub fn init_tracing(
    config: TraceConfig,
    extra_layers: Vec<Box<dyn Layer<Registry> + Send + Sync>>,
) -> TracingGuard {
    let filter = log_filter(&config);
    #[cfg(feature = "console")]
    let console_layer = tracing_subscriber::fmt::layer().json();

    #[cfg(feature = "otlp")]
    let (log_layer, log_provider) = {
        use opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge;
        use opentelemetry_otlp::{LogExporter, WithExportConfig as _, WithTonicConfig as _};
        use opentelemetry_sdk::logs::SdkLoggerProvider;
        let (endpoint, metadata) = otlp_grpc_config(config.otlp_endpoint, config.otlp_metadata);
        let exporter = LogExporter::builder()
            .with_tonic()
            .with_endpoint(endpoint)
            .with_metadata(metadata)
            .build()
            .expect("build log exporter");
        let provider = SdkLoggerProvider::builder()
            .with_batch_exporter(exporter)
            .with_resource(resource(config.otlp_service_name))
            .build();
        let layer = OpenTelemetryTracingBridge::new(&provider);
        (layer, provider)
    };

    #[cfg(feature = "otlp")]
    let (metrics_layer, metrics_provider) = {
        use opentelemetry_otlp::{MetricExporter, WithExportConfig as _, WithTonicConfig as _};
        use opentelemetry_sdk::metrics::{MeterProviderBuilder, PeriodicReader, Temporality};
        use tracing_opentelemetry::MetricsLayer;
        let (endpoint, metadata) = otlp_grpc_config(config.otlp_endpoint, config.otlp_metadata);
        // OTLP/gRPC：性能优先（HTTP/2 多路复用）。注意 OpenObserve 对 gRPC 直方图曾存 0 事件
        // （issue #12345，#12615 仅修 OTLP/JSON 路径）——若实测仍丢直方图，切回 HTTP+JSON
        // （git revert 或改 with_http + Protocol::HttpJson）。
        let exporter = MetricExporter::builder()
            .with_tonic()
            .with_endpoint(endpoint)
            .with_metadata(metadata)
            .with_temporality(Temporality::Cumulative)
            .build()
            .expect("build metrics exporter");
        let reader = PeriodicReader::builder(exporter)
            .with_interval(std::time::Duration::from_secs(30))
            .build();
        let provider = MeterProviderBuilder::default()
            .with_reader(reader)
            .with_resource(resource(config.otlp_service_name))
            .build();
        // 注册全局 meter provider：业务代码经 `opentelemetry::global::meter("slab")` 取仪表埋点。
        opentelemetry::global::set_meter_provider(provider.clone());
        let layer = MetricsLayer::new(provider.clone());
        (layer, provider)
    };

    #[cfg(feature = "otlp")]
    let (trace_layer, trace_provider) = {
        use opentelemetry::trace::TracerProvider as _;
        use opentelemetry_otlp::{WithExportConfig as _, WithTonicConfig as _};
        use opentelemetry_sdk::trace::{RandomIdGenerator, Sampler, SdkTracerProvider};

        let (endpoint, metadata) = otlp_grpc_config(config.otlp_endpoint, config.otlp_metadata);
        let exporter = opentelemetry_otlp::SpanExporter::builder()
            .with_tonic()
            .with_endpoint(endpoint)
            .with_metadata(metadata)
            .build()
            .expect("build trace exporter");

        let provider = SdkTracerProvider::builder()
            .with_sampler(Sampler::AlwaysOn)
            .with_resource(resource(config.otlp_service_name))
            .with_id_generator(RandomIdGenerator::default())
            .with_batch_exporter(exporter)
            .build();

        let trace_provider = provider.tracer(config.otlp_service_name.to_string());
        let layer = tracing_opentelemetry::layer().with_tracer(trace_provider);
        (layer, provider)
    };

    // per-layer filtering（tracing-subscriber 标准机制，非 hack）：各日志/桥接层独立挂
    // EnvFilter，互不干扰；extra 层（sqlx 指标层）放在最前，靠自身 on_event 过滤。
    // 不能把 EnvFilter 当全局 Layer 注册：`Layered::enabled` 是 AND 链，`sqlx::query=off`
    // 会全局掐断 sqlx 事件构造（指标层收不到事件）；per-layer 下 `Filtered::enabled`
    // 恒 true 不短路，各层独立裁决。
    let extra_stack: Box<dyn Layer<Registry> + Send + Sync> = extra_layers
        .into_iter()
        .reduce(|acc, layer| Box::new(acc.and_then(layer)))
        .unwrap_or_else(|| Box::new(tracing_subscriber::layer::Identity::new()));
    let builder = tracing_subscriber::registry().with(extra_stack);
    let builder = builder.with(RootcauseLayer.with_filter(filter.clone()));
    #[cfg(feature = "console")]
    let builder = builder.with(console_layer.with_filter(filter.clone()));

    #[cfg(feature = "otlp")]
    let builder = builder
        .with(log_layer.with_filter(filter.clone()))
        .with(metrics_layer.with_filter(filter.clone()))
        .with(trace_layer.with_filter(filter.clone()));

    builder.init();

    // rootcause-tracing 第二步：把 RootcauseLayer 捕获的 span 自动附加到错误报告。
    // 注意：hooks 为进程全局且只能安装一次，重复调用 init_tracing 会 panic（init 期可接受）。
    Hooks::new()
        .report_creation_hook(SpanCollector::new())
        .install()
        .expect("failed to install rootcause hooks");

    #[cfg(feature = "otlp")]
    #[allow(unreachable_code)]
    return TracingGuard::Otlp(OtlpGuard {
        log_provider,
        metrics_provider,
        trace_provider,
    });
    #[allow(unreachable_code)]
    TracingGuard::None
}

#[cfg(feature = "otlp")]
fn resource(service_name: &str) -> Resource {
    Resource::builder()
        .with_service_name(Value::String(StringValue::from(service_name.to_string())))
        .with_schema_url(
            [KeyValue::new(SERVICE_VERSION, env!("CARGO_PKG_VERSION"))],
            SCHEMA_URL,
        )
        .build()
}

#[cfg(feature = "otlp")]
pub enum TracingGuard {
    #[cfg(feature = "otlp")]
    Otlp(OtlpGuard),
    None,
}

#[cfg(feature = "otlp")]
pub struct OtlpGuard {
    log_provider: opentelemetry_sdk::logs::SdkLoggerProvider,
    metrics_provider: opentelemetry_sdk::metrics::SdkMeterProvider,
    trace_provider: opentelemetry_sdk::trace::SdkTracerProvider,
}

#[cfg(feature = "otlp")]
impl Drop for OtlpGuard {
    fn drop(&mut self) {
        eprintln!("Shutting down log provider");
        if let Err(err) = self.log_provider.shutdown() {
            eprintln!("{err:?}");
        }
        eprintln!("Shutting down metrics provider");
        if let Err(err) = self.metrics_provider.shutdown() {
            eprintln!("{err:?}");
        }
        eprintln!("Shutting down tracer provider");
        if let Err(err) = self.trace_provider.shutdown() {
            eprintln!("{err:?}");
        }
    }
}

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "init-time panics are acceptable; missing OTLP endpoint is a deployment error"
)]
#[cfg(feature = "otlp")]
use opentelemetry::{KeyValue, StringValue, Value};
#[cfg(feature = "otlp")]
use opentelemetry_otlp::tonic_types::metadata::MetadataMap;
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
/// `sqlx::query=off` 屏蔽 sqlx 逐条 SQL 日志噪音（查询耗时由 `SqlxQueryMetricsLayer` 消费，见 bin/server/metrics.rs）。
/// 显式设置 RUST_LOG 时以用户为准（不强制追加）。
fn log_filter(config: &TraceConfig) -> EnvFilter {
    EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new(format!(
            "{},otel::tracing=trace,sqlx::query=off",
            config.level
        ))
    })
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
        let budiler = LogExporter::builder().with_tonic();
        #[cfg(feature = "otlp_tls")]
        let budiler = budiler.with_tls_config(
            opentelemetry_otlp::tonic_types::transport::ClientTlsConfig::new().with_enabled_roots(),
        );
        let exporter = budiler
            .with_endpoint(config.otlp_endpoint)
            .with_metadata(metadata_from_json(config.otlp_metadata))
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
        let budiler = MetricExporter::builder().with_tonic();
        #[cfg(feature = "otlp_tls")]
        let budiler = budiler.with_tls_config(
            opentelemetry_otlp::tonic_types::transport::ClientTlsConfig::new().with_enabled_roots(),
        );
        let exporter = budiler
            .with_endpoint(config.otlp_endpoint)
            .with_metadata(metadata_from_json(config.otlp_metadata))
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

        let budiler = opentelemetry_otlp::SpanExporter::builder().with_tonic();
        #[cfg(feature = "otlp_tls")]
        let budiler = budiler.with_tls_config(
            opentelemetry_otlp::tonic_types::transport::ClientTlsConfig::new().with_enabled_roots(),
        );
        let exporter = budiler
            .with_endpoint(config.otlp_endpoint)
            .with_metadata(metadata_from_json(config.otlp_metadata))
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

    // 各层独立过滤：extra 层（如 sqlx 指标层）放在最前，自带 event_enabled 自过滤
    // （裸 Layer，不依赖 per-layer Filter 注册路径——经 Box<dyn Layer> 包装时 Filtered 的
    // filter 注册不可靠，实测收不到事件）；日志/桥接层统一挂 EnvFilter per-layer Filter——
    // 注意不能把 EnvFilter 当全局 Layer 注册，否则 `Layered::enabled` 的 AND 链会全局
    // 掐断被它拒绝的事件（sqlx 指标层将收不到任何事件）。
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
fn metadata_from_json(s: &str) -> MetadataMap {
    use std::str::FromStr as _;
    let map: std::collections::HashMap<String, String> =
        serde_json::from_str(s).expect("parse otlp_metadata");
    let mut m = MetadataMap::new();
    for (k, v) in map {
        let key = tonic::metadata::MetadataKey::from_str(&k).expect("metadata key");
        let value = tonic::metadata::MetadataValue::from_str(&v).expect("metadata value");
        m.insert(key, value);
    }
    m
}

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

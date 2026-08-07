use humantime::parse_duration;
use secrecy::SecretString;
use std::{str::FromStr, time::Duration};

use clap::Parser;

#[derive(Clone, Debug, Parser)]
#[command(version, about, long_about = None)]
pub struct Cli {
    /// Log level: trace, debug, info, warn, error
    #[arg(long, default_value = "info", env = "LOG_LEVEL")]
    pub log_level: String,

    /// Server instance is master
    #[arg(long, default_value = "false", env = "SERVER_MASTER")]
    pub server_master: bool,

    /// Scalar ui enabled
    #[arg(long, default_value = "false", env = "SCALAR_UI_ENABLED")]
    pub scalar_ui_enabled: bool,

    #[command(flatten)]
    pub server: ServerCli,

    #[command(flatten)]
    pub database: DatabaseCli,

    #[command(flatten)]
    pub jwt: JwtCli,

    #[command(flatten)]
    pub s3: S3Cli,

    #[command(flatten)]
    pub cache: CacheCli,

    #[command(flatten)]
    pub nats: NatsCli,

    #[command(flatten)]
    pub otlp: OtlpCli,
}

#[derive(Clone, Debug, clap::Args)]
pub struct ServerCli {
    #[arg(
        id = "listen_addr",
        long = "listen-addr",
        default_value = "0.0.0.0:8080",
        env = "LISTEN_ADDR"
    )]
    pub listen_addr: String,

    #[arg(
        id = "request_timeout",
        long = "request-timeout",
        default_value = "60s",
        env = "REQUEST_TIMEOUT"
    )]
    pub request_timeout: ReadableDuration,
}

#[derive(Clone, Debug, clap::Args)]
pub struct DatabaseCli {
    #[arg(
        id = "database_url",
        long = "database-url",
        env = "DATABASE_URL",
        value_parser = parse_secret_string
    )]
    pub url: SecretString,

    #[arg(
        id = "database_max_connections",
        long = "database-max-connections",
        default_value = "200",
        env = "DATABASE_MAX_CONNECTIONS"
    )]
    pub max_connections: u32,

    #[arg(
        id = "database_min_connections",
        long = "database-min-connections",
        default_value = "1",
        env = "DATABASE_MIN_CONNECTIONS"
    )]
    pub min_connections: u32,

    #[arg(
        id = "database_max_lifetime",
        long = "database-max-lifetime",
        default_value = "15min",
        env = "DATABASE_MAX_LIFETIME"
    )]
    pub max_lifetime: ReadableDuration,

    #[arg(
        id = "database_idle_timeout",
        long = "database-idle-timeout",
        default_value = "5min",
        env = "DATABASE_IDLE_TIMEOUT"
    )]
    pub idle_timeout: ReadableDuration,

    #[arg(
        id = "database_acquire_timeout",
        long = "database-acquire-timeout",
        default_value = "5s",
        env = "DATABASE_ACQUIRE_TIMEOUT"
    )]
    pub acquire_timeout: ReadableDuration,
}

#[derive(Clone, Debug, clap::Args)]
pub struct JwtCli {
    #[arg(
        id = "jwt_secret",
        long = "jwt-secret",
        env = "JWT_SECRET",
        value_parser = parse_secret_string
    )]
    pub secret: SecretString,

    #[arg(
        id = "access_token_ttl",
        long = "access-token-ttl",
        default_value = "15min",
        env = "ACCESS_TOKEN_TTL"
    )]
    pub access_ttl: ReadableDuration,

    #[arg(
        id = "refresh_token_ttl",
        long = "refresh-token-ttl",
        default_value = "30d",
        env = "REFRESH_TOKEN_TTL"
    )]
    pub refresh_ttl: ReadableDuration,
}

impl JwtCli {
    pub fn access_ttl_secs(&self) -> u64 {
        self.access_ttl.duration().as_secs()
    }

    pub fn refresh_ttl_secs(&self) -> u64 {
        self.refresh_ttl.duration().as_secs()
    }
}

#[derive(Clone, Debug, clap::Args)]
pub struct CacheCli {
    /// redb 数据文件路径（kv-redb 后端）
    #[arg(
        id = "cache_db_path",
        long = "cache-db-path",
        default_value = "data/cache.redb",
        env = "CACHE_DB_PATH"
    )]
    pub db_path: String,

    /// Redis 连接 URL（kv-redis 后端）
    #[arg(
        id = "redis_url",
        long = "redis-url",
        default_value = "redis://127.0.0.1:6379",
        env = "REDIS_URL"
    )]
    pub url: String,
}

#[derive(Clone, Debug, clap::Args)]
pub struct NatsCli {
    /// NATS 服务器地址（queue-nats 后端）
    #[arg(
        id = "nats_url",
        long = "nats-url",
        default_value = "nats://127.0.0.1:4222",
        env = "NATS_URL"
    )]
    pub url: String,

    #[arg(id = "nats_username", long = "nats-username", env = "NATS_USERNAME")]
    pub username: Option<String>,

    #[arg(id = "nats_password", long = "nats-password", env = "NATS_PASSWORD")]
    pub password: Option<String>,

    /// JetStream stream 名（自动 get_or_create）
    #[arg(
        id = "nats_stream_name",
        long = "nats-stream-name",
        default_value = "slab",
        env = "NATS_STREAM_NAME"
    )]
    pub stream_name: String,
}

#[derive(Clone, Debug, clap::Args)]
pub struct S3Cli {
    #[arg(id = "s3_endpoint", long = "s3-endpoint", env = "S3_ENDPOINT")]
    pub endpoint: String,

    #[arg(
        id = "s3_secret_id",
        long = "s3-secret-id",
        env = "S3_SECRET_ID",
        value_parser = parse_secret_string
    )]
    pub secret_id: SecretString,

    #[arg(
        id = "s3_secret_key",
        long = "s3-secret-key",
        env = "S3_SECRET_KEY",
        value_parser = parse_secret_string
    )]
    pub secret_key: SecretString,

    #[arg(id = "s3_bucket", long = "s3-bucket", env = "S3_BUCKET")]
    pub bucket: String,

    #[arg(id = "s3_domain", long = "s3-domain", env = "S3_DOMAIN")]
    pub domain: String,
}

#[derive(Clone, Debug, clap::Args)]
pub struct OtlpCli {
    #[arg(id = "otlp_endpoint", long = "otlp-endpoint", env = "OTLP_ENDPOINT")]
    pub endpoint: String,

    #[arg(
        id = "otlp_service_name",
        long = "otlp-service-name",
        env = "OTLP_SERVICE_NAME"
    )]
    pub service_name: String,

    #[arg(
        id = "otlp_metadata",
        long = "otlp-metadata",
        env = "OTLP_METADATA",
        value_parser = parse_secret_string
    )]
    pub metadata: SecretString,
}

#[derive(Debug, Clone)]
pub struct ReadableDuration(Duration);

impl ReadableDuration {
    pub fn duration(&self) -> Duration {
        self.0
    }
}

impl FromStr for ReadableDuration {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        parse_duration(s)
            .map(ReadableDuration)
            .map_err(|e| format!("invalid readable duration '{}': {}", s, e))
    }
}

fn parse_secret_string(s: &str) -> std::result::Result<SecretString, String> {
    Ok(SecretString::from(s))
}

use std::time::Duration;

use rootcause::Result;
use secrecy::{ExposeSecret, SecretString};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};

#[derive(Debug)]
pub struct DbConfig {
    pub url: SecretString,
    pub max_connections: u32,
    pub min_connections: u32,
    pub max_lifetime: Duration,
    pub idle_timeout: Duration,
    pub acquire_timeout: Duration,
}

pub type PgPool = sqlx::PgPool;

pub async fn connect(config: DbConfig) -> Result<PgPool> {
    let pool_connection_options: PgConnectOptions = config.url.expose_secret().parse()?;
    let pool = PgPoolOptions::new()
        .max_connections(config.max_connections)
        .min_connections(config.min_connections)
        .max_lifetime(config.max_lifetime)
        .idle_timeout(config.idle_timeout)
        .acquire_timeout(config.acquire_timeout)
        .connect_with(pool_connection_options)
        .await?;
    let mut conn = pool.acquire().await?;
    let info = sqlx::query!(
        r#"SELECT version() AS "version!", current_setting('TimeZone') AS "timezone!""#,
    )
    .fetch_one(&mut *conn)
    .await?;
    tracing::info!(
        "postgresql: {} connected, timezone: {}",
        info.version,
        info.timezone
    );
    drop(conn);
    Ok(pool)
}

use clap::Parser as _;
use rootcause::Result;
use secrecy::ExposeSecret;
use trace_kit::{TraceConfig, init_tracing};
use tracing::info;

use crate::cli::Cli;

mod api_doc;
mod cli;
mod config;
mod gc_jobs;
mod modules;
mod router;
mod server;
mod shutdown;

#[cfg(test)]
#[path = "arch_test.rs"]
mod arch_test;

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv_override().ok();
    let cli: Cli = Cli::parse();
    let _guard = init_tracing(TraceConfig::new(
        &cli.log_level,
        &cli.otlp.service_name,
        &cli.otlp.endpoint,
        cli.otlp.metadata.expose_secret(),
    ));
    info!("{:?}", &cli);
    server::serve(cli).await?;
    Ok(())
}

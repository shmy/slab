use std::net::SocketAddr;

use axum::Router;
use lazy_limit::{Duration, RuleConfig};
use module::ModuleRegistrar;
use rootcause::{Report, Result, report};
use tokio::net::TcpListener;
use tokio::sync::watch::Receiver;

use crate::cli::Cli;
use crate::config::build_app_ctx;
use crate::gc_jobs::{BusGc, KvGc};
use crate::modules::MODULES;
use crate::router::build;
use crate::shutdown::{ShutdownCoordinator, shutdown_signal};
use worker::WorkerManager;

pub async fn serve(cli: Cli) -> Result<()> {
    let listener = TcpListener::bind(&cli.server.listen_addr).await?;
    let addr = listener.local_addr()?;

    let state = build_app_ctx(&cli).await?;
    migration::run_migrations(&state.pg_pool).await?;
    for module in MODULES {
        module.on_start(&state).await?;
        tracing::info!("module {} started", module.name());
    }

    let router = build(
        state.clone(),
        cli.server.request_timeout.duration(),
        cli.scalar_ui_enabled,
    );

    let shutdown = ShutdownCoordinator::new();
    let registrar = {
        let mut registrar = ModuleRegistrar::new(state.clone());
        registrar.scheduler.add(KvGc);
        registrar.scheduler.add(BusGc);
        for module in MODULES {
            module.register(&mut registrar);
        }
        registrar
    };
    let frozen_registry = registrar.bus.freeze();

    // 后台任务 worker：收编域模块注册的 Job handler，进程内消费（同 dispatcher）。
    let mut worker_manager = WorkerManager::new(state.jobs.clone(), state.clone());
    worker_manager.register_all(&registrar.jobs);

    let server_fut = tokio::spawn(start_http_server(listener, router, shutdown.subscribe()));
    let dispatcher_fut = tokio::spawn(start_dispatcher(
        state.bus.clone(),
        state.clone(),
        frozen_registry,
        shutdown.subscribe(),
    ));
    let scheduler_shutdown = shutdown.subscribe();
    let scheduler_fut = tokio::spawn(async move {
        registrar
            .scheduler
            .start(cli.server_master, scheduler_shutdown)
            .await
    });

    let worker_shutdown = shutdown.subscribe();
    let worker_fut = tokio::spawn(async move { worker_manager.run(worker_shutdown).await });

    tracing::info!("🚀 Server is running on http://{addr}");

    shutdown_signal().await;
    shutdown.broadcast_shutdown();

    let (http_join, dispatcher_join, scheduler_join, worker_join) =
        tokio::join!(server_fut, dispatcher_fut, scheduler_fut, worker_fut);
    join_task("HTTP server", http_join)?;
    join_task("event_bus dispatcher", dispatcher_join)?;
    join_task("cron scheduler", scheduler_join)?;
    join_task("job worker", worker_join)?;

    state.clear().await;
    tracing::info!("👋 Goodbye!");
    Ok(())
}

async fn start_http_server(listener: TcpListener, app: Router, rx: Receiver<bool>) -> Result<()> {
    lazy_limit::init_rate_limiter!(
        default: RuleConfig::new(Duration::seconds(1), 24),
        routes: [
            // ("/api/v1/auth/send_code", RuleConfig::new(Duration::minutes(1), 3)),
        ]
    )
    .await;
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(async move {
        crate::shutdown::until_shutdown(rx).await;
        tracing::info!("HTTP server draining connections...");
    })
    .await?;
    Ok(())
}

async fn start_dispatcher(
    backend: event_bus::EventBus,
    ctx: appctx::AppCtx,
    registry: event_bus::FrozenRegistry<appctx::AppCtx>,
    rx: Receiver<bool>,
) -> Result<()> {
    backend.run_dispatcher(ctx, registry, rx).await
}

fn join_task(
    name: &'static str,
    join: std::result::Result<std::result::Result<(), Report>, tokio::task::JoinError>,
) -> Result<()> {
    match join {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => Err(e),
        Err(e) => {
            if e.is_panic() {
                std::panic::resume_unwind(e.into_panic());
            }
            Err(report!("{name} task join failed: {e}"))
        }
    }
}

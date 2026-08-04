use tokio::sync::watch::{self, Receiver, Sender};

pub struct ShutdownCoordinator {
    tx: Sender<bool>,
}

impl ShutdownCoordinator {
    pub fn new() -> Self {
        let (tx, _) = watch::channel(false);
        Self { tx }
    }

    pub fn subscribe(&self) -> Receiver<bool> {
        self.tx.subscribe()
    }

    pub fn broadcast_shutdown(&self) {
        let _ = self.tx.send(true);
    }
}

pub async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
        tracing::warn!("Received Ctrl+C");
    };

    #[cfg(unix)]
    let terminate = async {
        use tokio::signal;
        use tracing::warn;

        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("Failed to install signal handler")
            .recv()
            .await;
        warn!("Received SIGTERM");
    };

    #[cfg(unix)]
    let quit = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::quit())
            .expect("Failed to install signal handler")
            .recv()
            .await;
        tracing::warn!("Received SIGQUIT");
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    #[cfg(not(unix))]
    let quit = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
        _ = quit => {},
    }
}

pub async fn until_shutdown(mut rx: Receiver<bool>) {
    if *rx.borrow() {
        return;
    }
    let _ = rx.changed().await;
}

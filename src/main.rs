mod config;
mod domain;
mod http;
mod store;

use std::sync::Arc;

use tokio::net::TcpListener;
use tracing_subscriber::{EnvFilter, fmt};

use crate::{
    config::AppConfig,
    http::{AppState, router},
    store::AppStore,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();

    let config = AppConfig::from_env()?;
    let app = router(AppState {
        environment: config.environment.clone(),
        store: Arc::new(AppStore::new()),
    });
    let listener = TcpListener::bind(config.bind_addr).await?;

    tracing::info!(
        bind_addr = %config.bind_addr,
        environment = %config.environment,
        "starting LiquidLane Core"
    );

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("liquidlane_core=debug,tower_http=info,info"));

    fmt().with_env_filter(filter).init();
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install terminate signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}

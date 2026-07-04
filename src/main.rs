mod ckb_rpc;
mod config;
mod deployment;
mod domain;
mod fiber;
mod http;
mod store;

use std::sync::Arc;

use tokio::net::TcpListener;
use tracing_subscriber::{EnvFilter, fmt};

use crate::{
    ckb_rpc::CkbRpcClient,
    config::AppConfig,
    fiber::FiberClient,
    http::{AppState, router},
    store::AppStore,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    init_tracing();

    let config = AppConfig::from_env()?;
    let fiber = FiberClient::new(
        config.fiber_rpc_url.clone(),
        config.fiber_rpc_auth_token.clone(),
    );
    let ckb_rpc = config
        .ckb_rpc_url
        .clone()
        .map(|url| CkbRpcClient::new(url, config.ckb_accept_pending_txs));
    let store = AppStore::load(
        config.data_path.clone(),
        fiber.clone(),
        config.vault.clone(),
        ckb_rpc,
        config.require_ckb_rpc,
    )
    .await?;
    let app = router(AppState {
        environment: config.environment.clone(),
        vault: config.vault.clone(),
        ckb_script_build_dir: config.ckb_script_build_dir.clone(),
        store: Arc::new(store),
    });
    let listener = TcpListener::bind(config.bind_addr).await?;

    tracing::info!(
        bind_addr = %config.bind_addr,
        environment = %config.environment,
        data_path = %config.data_path.display(),
        fiber_rpc_configured = fiber.is_configured(),
        ckb_rpc_configured = config.ckb_rpc_url.is_some(),
        ckb_rpc_required = config.require_ckb_rpc,
        vault_asset = %config.vault.asset,
        vault_network = %config.vault.network,
        ckb_script_build_dir = %config.ckb_script_build_dir.display(),
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

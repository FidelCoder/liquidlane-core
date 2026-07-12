mod ckb_rpc;
mod config;
mod deployment;
mod domain;
mod fiber;
mod http;
mod store;

use std::{sync::Arc, time::Duration};

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
    let store = Arc::new(
        AppStore::load(
            config.data_path.clone(),
            fiber.clone(),
            config.vault.clone(),
            ckb_rpc,
            config.require_ckb_rpc,
            config.executor_enabled,
            config.executor_poll_interval_ms,
            config.executor_max_retries,
            config.executor_funding_mode.clone(),
            config.vault_funding_builder_enabled,
            config.vault_funding_signer_enabled,
        )
        .await?,
    );
    if config.executor_enabled {
        spawn_executor_worker(store.clone(), config.executor_poll_interval_ms);
    }
    let app = router(AppState {
        environment: config.environment.clone(),
        ckb_script_build_dir: config.ckb_script_build_dir.clone(),
        fiber_rpc_configured: fiber.is_configured(),
        ckb_rpc_configured: config.ckb_rpc_url.is_some(),
        cors_allowed_origin: config.cors_allowed_origin.clone(),
        store: store.clone(),
    });
    let listener = TcpListener::bind(config.bind_addr).await?;

    tracing::info!(
        bind_addr = %config.bind_addr,
        environment = %config.environment,
        data_path = %config.data_path.display(),
        fiber_rpc_configured = fiber.is_configured(),
        ckb_rpc_configured = config.ckb_rpc_url.is_some(),
        ckb_rpc_required = config.require_ckb_rpc,
        executor_enabled = config.executor_enabled,
        executor_funding_mode = %config.executor_funding_mode,
        vault_asset = %config.vault.asset,
        vault_network = %config.vault.network,
        ckb_script_build_dir = %config.ckb_script_build_dir.display(),
        cors_allowed_origin = ?config.cors_allowed_origin,
        "starting LiquidLane Core"
    );

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

fn spawn_executor_worker(store: Arc<AppStore>, poll_interval_ms: u64) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_millis(poll_interval_ms.max(1_000)));
        loop {
            ticker.tick().await;
            match store.sync_fiber_channels().await {
                Ok(changed) if changed > 0 => {
                    tracing::info!(changed, "synced Fiber channel states from watcher");
                }
                Ok(_) => {}
                Err(error) => {
                    tracing::warn!(error = %error, "failed to sync Fiber channel status");
                }
            }
            match store.release_expired_requests().await {
                Ok(released) if released > 0 => {
                    tracing::info!(released, "released expired LiquidLane reservations");
                }
                Ok(_) => {}
                Err(error) => {
                    tracing::warn!(error = %error, "failed to release expired reservations");
                }
            }
        }
    });
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

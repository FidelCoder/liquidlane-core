mod auth;
mod error;
mod routes;
#[cfg(test)]
mod tests;

use std::{path::PathBuf, sync::Arc};

use axum::{
    Router,
    routing::{get, post},
};
use tower_http::{cors::CorsLayer, trace::TraceLayer};

use crate::{domain::VaultConfig, store::AppStore};

pub(crate) use auth::AuthedUser;
pub(crate) use error::ApiError;

#[derive(Clone)]
pub struct AppState {
    pub environment: String,
    pub store: Arc<AppStore>,
    pub vault: VaultConfig,
    pub ckb_script_build_dir: PathBuf,
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(routes::health))
        .route("/auth/challenge", post(routes::create_challenge))
        .route("/auth/connect", post(routes::connect_wallet))
        .route("/auth/verify", post(routes::verify_wallet))
        .route("/me", get(routes::me))
        .route("/vault", get(routes::vault))
        .route("/deployment/package", get(routes::deployment_package))
        .route("/vault/supply/intents", post(routes::create_supply_intent))
        .route(
            "/vault/withdrawals/intents",
            post(routes::create_withdrawal_intent),
        )
        .route(
            "/vault/withdrawals/{id}/settle",
            post(routes::settle_withdrawal),
        )
        .route("/vault/fees/claims", post(routes::create_fee_claim))
        .route("/dashboard", get(routes::dashboard))
        .route("/deposits", post(routes::create_deposit))
        .route("/liquidity/quote", post(routes::create_quote))
        .route(
            "/liquidity/requests",
            post(routes::create_liquidity_request),
        )
        .route(
            "/liquidity/requests/{id}/deploy",
            post(routes::deploy_liquidity),
        )
        .with_state(state)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
}

mod auth;
mod error;
mod ops;
mod routes;
#[cfg(test)]
mod tests;

use std::{path::PathBuf, sync::Arc};

use axum::{
    Router,
    http::{HeaderValue, Method, header},
    routing::{get, post},
};
use tower_http::{cors::CorsLayer, trace::TraceLayer};

use crate::store::AppStore;

pub(crate) use auth::AuthedUser;
pub(crate) use error::ApiError;

#[derive(Clone)]
pub struct AppState {
    pub environment: String,
    pub store: Arc<AppStore>,
    pub ckb_script_build_dir: PathBuf,
    pub fiber_rpc_configured: bool,
    pub ckb_rpc_configured: bool,
    pub cors_allowed_origin: Option<String>,
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(ops::health))
        .route("/monitoring", get(ops::monitoring))
        .route("/internal/executor/health", get(ops::executor_health))
        .route("/internal/executor/jobs", get(ops::executor_jobs))
        .route(
            "/internal/executor/external-funding",
            get(ops::external_funding),
        )
        .route(
            "/internal/executor/jobs/{id}/retry",
            post(ops::retry_executor_job),
        )
        .route(
            "/internal/executor/release-expired",
            post(ops::release_expired_requests),
        )
        .route("/internal/state/export", get(ops::state_export))
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
        .route(
            "/vault/fees/claims/{id}/settle",
            post(routes::settle_fee_claim),
        )
        .route("/dashboard", get(routes::dashboard))
        .route("/deposits", post(routes::create_deposit))
        .route("/liquidity/quote", post(routes::create_quote))
        .route(
            "/liquidity/request/intents",
            post(routes::create_request_intent),
        )
        .route(
            "/liquidity/requests",
            post(routes::create_liquidity_request),
        )
        .route(
            "/liquidity/requests/{id}/peer",
            post(routes::attach_fiber_peer),
        )
        .route(
            "/liquidity/requests/{id}/deploy",
            post(routes::deploy_liquidity),
        )
        .with_state(state.clone())
        .layer(cors_layer(state.cors_allowed_origin.as_deref()))
        .layer(TraceLayer::new_for_http())
}

fn cors_layer(allowed_origin: Option<&str>) -> CorsLayer {
    let Some(origin) = allowed_origin else {
        return CorsLayer::permissive();
    };
    let Ok(origin) = HeaderValue::from_str(origin) else {
        return CorsLayer::permissive();
    };

    CorsLayer::new()
        .allow_origin(origin)
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE])
}

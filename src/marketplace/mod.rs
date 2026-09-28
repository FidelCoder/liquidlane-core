pub mod auth;
pub mod chain;
pub mod config;
pub mod connector_api;
pub mod crypto;
pub mod db;
pub mod error;
pub mod evidence;
mod heartbeat;
pub mod joyid;
pub mod model;
pub mod nodes;
mod offers;
pub mod orders;
mod payments;
mod receipts;
pub mod settlement;

use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, State},
    http::{HeaderValue, Method, header},
    routing::{get, post},
};
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};
use tower_http::{cors::CorsLayer, trace::TraceLayer};

pub const FIBER_FUNDING_CODE_HASH: &str =
    "0x6c67887fe201ee0c7853f1682c0b77c0e6214044c156c7558269390a8afa6d7c";

#[derive(Clone)]
pub struct Market {
    pub config: Arc<config::Config>,
    pub db: Arc<db::Db>,
    pub client: reqwest::Client,
}
impl Market {
    pub fn new(config: config::Config) -> anyhow::Result<Self> {
        let database = db::Db::open(&config.database)?;
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(20))
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        Ok(Self {
            config: Arc::new(config),
            db: Arc::new(database),
            client,
        })
    }
}
pub fn router(state: Market) -> anyhow::Result<Router> {
    let cors = CorsLayer::new()
        .allow_origin(state.config.origin.parse::<HeaderValue>()?)
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION]);
    Ok(Router::new()
        .route("/health", get(health))
        .route("/auth/challenge", post(auth::challenge))
        .route("/auth/verify", post(auth::verify))
        .route("/auth/logout", post(auth::logout))
        .route("/market/offers", get(offers::list).post(offers::create))
        .route("/market/offers/{id}/disable", post(offers::disable))
        .route("/market/dashboard", get(orders::dashboard))
        .route("/market/nodes/pair", post(nodes::pair))
        .route("/market/nodes/pair/{id}", get(nodes::pairing_status))
        .route("/market/orders", post(orders::create))
        .route("/market/orders/{id}/accept", post(orders::accept))
        .route("/market/orders/{id}/cancel", post(orders::cancel))
        .route(
            "/market/orders/{id}/verify",
            post(evidence::verify_delivery),
        )
        .route("/market/orders/{id}/fee", post(payments::settle))
        .route("/market/orders/{id}/waive-fee", post(payments::waive))
        .route("/market/orders/{id}/receipt", get(receipts::get))
        .route("/connector/register", post(nodes::register))
        .route("/connector/heartbeat", post(nodes::heartbeat))
        .route("/connector/orders", get(connector_api::orders))
        .route("/connector/orders/{id}/quote", post(connector_api::quote))
        .route("/connector/orders/{id}/start", post(connector_api::start))
        .route(
            "/connector/orders/{id}/failure",
            post(connector_api::failure),
        )
        .route("/connector/orders/{id}/evidence", post(evidence::report))
        .route("/connector/orders/{id}/probe", post(connector_api::probe))
        .with_state(state)
        .layer(DefaultBodyLimit::max(64 * 1024))
        .layer(cors)
        .layer(TraceLayer::new_for_http()))
}
async fn health(State(state): State<Market>) -> Json<Value> {
    Json(
        json!({"status":"ok","service":"liquidlane-marketplace","protocol":model::PROTOCOL,"network":"testnet","custody":"participant_owned_nodes","fiber_version":state.config.fiber_version,"service_model":"initial_receive_capacity","guaranteed_lease":false}),
    )
}
pub async fn serve() -> anyhow::Result<()> {
    let state = Market::new(config::Config::from_env()?)?;
    chain::verify_network(&state.client, &state.config.ckb_rpc).await?;
    let worker = state.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(10));
        loop {
            interval.tick().await;
            if let Err(error) = orders::expire(&worker) {
                tracing::error!(%error,"marketplace expiration failed");
            }
        }
    });
    let listener = tokio::net::TcpListener::bind(&state.config.bind).await?;
    tracing::info!(bind=%state.config.bind,"starting CKB testnet marketplace; vault routes disabled");
    axum::serve(listener, router(state)?)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}

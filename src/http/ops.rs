use axum::{
    Json,
    extract::{Path, State},
    response::IntoResponse,
};
use serde::Serialize;
use uuid::Uuid;

use super::{ApiError, AppState, AuthedUser};
use crate::domain::{User, UserRole};
use crate::store::{CoreStateExport, ExecutorHealth};

#[derive(Serialize)]
pub(super) struct HealthResponse {
    status: &'static str,
    service: &'static str,
    environment: String,
    fiber_rpc_configured: bool,
    ckb_rpc_configured: bool,
    ckb_network: String,
    vault_configured: bool,
    beta_ready: bool,
    executor_enabled: bool,
    executor_funding_mode: String,
    executor_queued_requests: usize,
    executor_pending_handoffs: usize,
}

pub(super) async fn health(State(state): State<AppState>) -> Json<HealthResponse> {
    let vault = state.store.vault_config().await;
    let beta_ready = beta_ready(&state, &vault.network, vault.configured);
    let executor = state.store.executor_health().await;

    Json(HealthResponse {
        status: "ok",
        service: "liquidlane-core",
        environment: state.environment,
        fiber_rpc_configured: state.fiber_rpc_configured,
        ckb_rpc_configured: state.ckb_rpc_configured,
        ckb_network: vault.network,
        vault_configured: vault.configured,
        beta_ready,
        executor_enabled: executor.enabled,
        executor_funding_mode: executor.funding_mode,
        executor_queued_requests: executor.queued_requests,
        executor_pending_handoffs: executor.pending_handoffs,
    })
}

#[derive(Serialize)]
pub(super) struct MonitoringResponse {
    status: &'static str,
    environment: String,
    beta_ready: bool,
    vault_configured: bool,
    ckb_rpc_configured: bool,
    fiber_rpc_configured: bool,
    executor: ExecutorHealth,
    state: CoreStateExport,
}

pub(super) async fn monitoring(State(state): State<AppState>) -> Json<MonitoringResponse> {
    let vault = state.store.vault_config().await;
    let beta_ready = beta_ready(&state, &vault.network, vault.configured);
    Json(MonitoringResponse {
        status: "ok",
        environment: state.environment,
        beta_ready,
        vault_configured: vault.configured,
        ckb_rpc_configured: state.ckb_rpc_configured,
        fiber_rpc_configured: state.fiber_rpc_configured,
        executor: state.store.executor_health().await,
        state: state.store.state_export_summary().await,
    })
}

pub(super) async fn executor_health(
    State(state): State<AppState>,
    AuthedUser(user): AuthedUser,
) -> Result<impl IntoResponse, ApiError> {
    require_internal_operator(&user)?;
    Ok(Json(state.store.executor_health().await))
}

pub(super) async fn executor_jobs(
    State(state): State<AppState>,
    AuthedUser(user): AuthedUser,
) -> Result<impl IntoResponse, ApiError> {
    require_internal_operator(&user)?;
    Ok(Json(state.store.executor_jobs().await))
}

pub(super) async fn retry_executor_job(
    State(state): State<AppState>,
    AuthedUser(user): AuthedUser,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    require_internal_operator(&user)?;
    Ok(Json(state.store.retry_executor_job(id).await?))
}

#[derive(Serialize)]
pub(super) struct ReleaseExpiredResponse {
    released: usize,
}

pub(super) async fn release_expired_requests(
    State(state): State<AppState>,
    AuthedUser(user): AuthedUser,
) -> Result<impl IntoResponse, ApiError> {
    require_internal_operator(&user)?;
    Ok(Json(ReleaseExpiredResponse {
        released: state.store.release_expired_requests().await?,
    }))
}

pub(super) async fn state_export(
    State(state): State<AppState>,
    AuthedUser(user): AuthedUser,
) -> Result<impl IntoResponse, ApiError> {
    require_internal_operator(&user)?;
    Ok(Json(state.store.state_export_summary().await))
}

fn beta_ready(state: &AppState, network: &str, vault_configured: bool) -> bool {
    matches!(
        network.trim().to_ascii_lowercase().as_str(),
        "testnet" | "ckb-testnet" | "pudge" | "pudge-testnet"
    ) && vault_configured
        && state.ckb_rpc_configured
        && state.fiber_rpc_configured
}

fn require_internal_operator(user: &User) -> Result<(), ApiError> {
    if user.role == UserRole::Operator {
        Ok(())
    } else {
        Err(ApiError::unauthorized(
            "executor operations are internal to LiquidLane",
        ))
    }
}

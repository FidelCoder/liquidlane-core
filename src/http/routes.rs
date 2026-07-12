use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{ApiError, AppState, AuthedUser};
use crate::domain::{
    AttachFiberPeerRequest, ChallengeRequest, ConnectWalletRequest, CreateDepositRequest,
    CreateFeeClaimRequest, CreateLiquidityRequest, CreateSupplyIntentRequest,
    CreateWithdrawalIntentRequest, SettleFeeClaimRequest, SettleWithdrawalRequest, UserRole,
    VaultConfig, VerifyWalletRequest,
};

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
    let is_testnet = matches!(
        vault.network.trim().to_ascii_lowercase().as_str(),
        "testnet" | "ckb-testnet" | "pudge" | "pudge-testnet"
    );
    let beta_ready =
        is_testnet && vault.configured && state.ckb_rpc_configured && state.fiber_rpc_configured;
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

pub(super) async fn executor_health(
    State(state): State<AppState>,
    AuthedUser(user): AuthedUser,
) -> Result<impl IntoResponse, ApiError> {
    if user.role != UserRole::Operator {
        return Err(ApiError::unauthorized(
            "executor health is internal to LiquidLane operations",
        ));
    }
    Ok(Json(state.store.executor_health().await))
}

pub(super) async fn create_challenge(
    State(state): State<AppState>,
    Json(request): Json<ChallengeRequest>,
) -> Result<impl IntoResponse, ApiError> {
    Ok((
        StatusCode::CREATED,
        Json(state.store.create_challenge(request).await?),
    ))
}

pub(super) async fn connect_wallet(
    State(state): State<AppState>,
    Json(request): Json<ConnectWalletRequest>,
) -> Result<impl IntoResponse, ApiError> {
    Ok((
        StatusCode::CREATED,
        Json(state.store.connect_wallet(request).await?),
    ))
}

pub(super) async fn verify_wallet(
    State(state): State<AppState>,
    Json(request): Json<VerifyWalletRequest>,
) -> Result<impl IntoResponse, ApiError> {
    Ok((
        StatusCode::CREATED,
        Json(state.store.verify_wallet(request).await?),
    ))
}

pub(super) async fn me(AuthedUser(user): AuthedUser) -> impl IntoResponse {
    Json(crate::domain::UserProfile::from(&user))
}

pub(super) async fn vault(State(state): State<AppState>) -> Json<VaultConfig> {
    Json(state.store.vault_config().await)
}

pub(super) async fn deployment_package(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, ApiError> {
    Ok(Json(
        crate::deployment::load_script_package(&state.ckb_script_build_dir).await?,
    ))
}

#[derive(Deserialize)]
pub(super) struct DashboardQuery {
    asset: Option<String>,
}

pub(super) async fn dashboard(
    State(state): State<AppState>,
    AuthedUser(user): AuthedUser,
    Query(query): Query<DashboardQuery>,
) -> impl IntoResponse {
    Json(state.store.dashboard(&user, query.asset).await)
}

pub(super) async fn create_supply_intent(
    State(state): State<AppState>,
    AuthedUser(user): AuthedUser,
    Json(request): Json<CreateSupplyIntentRequest>,
) -> Result<impl IntoResponse, ApiError> {
    Ok((
        StatusCode::CREATED,
        Json(state.store.create_supply_intent(&user, request).await?),
    ))
}

pub(super) async fn create_withdrawal_intent(
    State(state): State<AppState>,
    AuthedUser(user): AuthedUser,
    Json(request): Json<CreateWithdrawalIntentRequest>,
) -> Result<impl IntoResponse, ApiError> {
    Ok((
        StatusCode::CREATED,
        Json(state.store.create_withdrawal_intent(&user, request).await?),
    ))
}

pub(super) async fn settle_withdrawal(
    State(state): State<AppState>,
    AuthedUser(user): AuthedUser,
    Path(id): Path<Uuid>,
    Json(request): Json<SettleWithdrawalRequest>,
) -> Result<impl IntoResponse, ApiError> {
    Ok(Json(
        state.store.settle_withdrawal(&user, id, request).await?,
    ))
}

pub(super) async fn create_fee_claim(
    State(state): State<AppState>,
    AuthedUser(user): AuthedUser,
    Json(request): Json<CreateFeeClaimRequest>,
) -> Result<impl IntoResponse, ApiError> {
    Ok((
        StatusCode::CREATED,
        Json(state.store.create_fee_claim(&user, request).await?),
    ))
}

pub(super) async fn settle_fee_claim(
    State(state): State<AppState>,
    AuthedUser(user): AuthedUser,
    Path(id): Path<Uuid>,
    Json(request): Json<SettleFeeClaimRequest>,
) -> Result<impl IntoResponse, ApiError> {
    Ok(Json(
        state.store.settle_fee_claim(&user, id, request).await?,
    ))
}

pub(super) async fn create_deposit(
    State(state): State<AppState>,
    AuthedUser(user): AuthedUser,
    Json(request): Json<CreateDepositRequest>,
) -> Result<impl IntoResponse, ApiError> {
    Ok((
        StatusCode::CREATED,
        Json(state.store.create_deposit(&user, request).await?),
    ))
}

pub(super) async fn create_quote(
    State(state): State<AppState>,
    AuthedUser(user): AuthedUser,
    Json(request): Json<CreateLiquidityRequest>,
) -> Result<impl IntoResponse, ApiError> {
    Ok(Json(state.store.quote(&user, &request).await?))
}

pub(super) async fn create_request_intent(
    State(state): State<AppState>,
    AuthedUser(user): AuthedUser,
    Json(request): Json<CreateLiquidityRequest>,
) -> Result<impl IntoResponse, ApiError> {
    Ok((
        StatusCode::CREATED,
        Json(state.store.create_request_intent(&user, request).await?),
    ))
}

pub(super) async fn create_liquidity_request(
    State(state): State<AppState>,
    AuthedUser(user): AuthedUser,
    Json(request): Json<CreateLiquidityRequest>,
) -> Result<impl IntoResponse, ApiError> {
    Ok((
        StatusCode::CREATED,
        Json(state.store.create_liquidity_request(&user, request).await?),
    ))
}

pub(super) async fn attach_fiber_peer(
    State(state): State<AppState>,
    AuthedUser(user): AuthedUser,
    Path(id): Path<Uuid>,
    Json(request): Json<AttachFiberPeerRequest>,
) -> Result<impl IntoResponse, ApiError> {
    Ok(Json(
        state.store.attach_fiber_peer(&user, id, request).await?,
    ))
}

pub(super) async fn deploy_liquidity(
    State(state): State<AppState>,
    AuthedUser(user): AuthedUser,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    Ok(Json(state.store.deploy_liquidity(&user, id).await?))
}

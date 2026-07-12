use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExternalFundingIntentStatus {
    BuilderRequired,
    ReadyForSigning,
    FundingSubmitted,
    ChannelActive,
    Failed,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExternalFundingIntent {
    pub id: Uuid,
    pub request_id: Uuid,
    pub merchant_id: Uuid,
    pub merchant_name: String,
    pub ckb_address: String,
    pub asset: String,
    pub amount: u64,
    #[serde(default)]
    pub request_tx_hash: Option<String>,
    #[serde(default)]
    pub request_cell_out_point: Option<String>,
    #[serde(default)]
    pub fiber_peer_pubkey: Option<String>,
    #[serde(default)]
    pub fiber_peer_address: Option<String>,
    pub status: ExternalFundingIntentStatus,
    #[serde(default)]
    pub blockers: Vec<String>,
    #[serde(default)]
    pub funding_tx_hash: Option<String>,
    #[serde(default)]
    pub funding_out_point: Option<String>,
    #[serde(default)]
    pub fiber_ref: Option<String>,
    pub note: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ExternalFundingPreview {
    pub request_id: Uuid,
    pub amount: u64,
    pub asset: String,
    pub fiber_peer_pubkey: Option<String>,
    pub request_tx_hash: Option<String>,
    pub request_cell_out_point: Option<String>,
    pub ready: bool,
    pub blockers: Vec<String>,
    pub next_action: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct ExternalFundingPlan {
    pub request_id: Uuid,
    pub amount: u64,
    pub asset: String,
    pub vault_cell_out_point: Option<String>,
    pub request_cell_out_point: Option<String>,
    pub funding_lock_target: Option<String>,
    pub required_signer: String,
    pub unsigned_tx_available: bool,
    pub ready_for_signing: bool,
    pub ready_for_submission: bool,
    pub blockers: Vec<String>,
    pub next_action: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ExternalFundingSubmitRequest {
    pub tx_hash: String,
    #[serde(default)]
    pub funding_out_point: Option<String>,
    #[serde(default)]
    pub signed_tx: Option<serde_json::Value>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ExternalFundingSubmitResponse {
    pub request: super::LiquidityRequest,
    pub intent: ExternalFundingIntent,
}

#[derive(Clone, Debug, Serialize)]
pub struct ExternalFundingWatcherState {
    pub funding_required: usize,
    pub funding_submitted: usize,
    pub pending_fiber_channel: usize,
    pub channel_open: usize,
    pub failed: usize,
    pub released_or_settled: usize,
    pub release_candidates: usize,
    pub open_jobs: usize,
    pub last_event_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ExternalFundingReadiness {
    pub supported: bool,
    pub ready: bool,
    pub funding_mode: String,
    pub vault_configured: bool,
    pub fiber_rpc_configured: bool,
    pub v2_scripts_configured: bool,
    pub funding_tx_builder_ready: bool,
    pub funding_signer_ready: bool,
    pub blockers: Vec<String>,
    pub next_action: String,
}

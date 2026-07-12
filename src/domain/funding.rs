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
    pub fiber_ref: Option<String>,
    pub note: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
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
    pub blockers: Vec<String>,
}

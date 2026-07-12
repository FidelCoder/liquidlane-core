use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use super::CkbScript;

#[derive(Clone, Debug, Deserialize)]
pub struct CreateLiquidityRequest {
    pub asset: String,
    pub amount: u64,
    pub duration_days: u16,
    #[serde(default)]
    pub intent_id: Option<Uuid>,
    #[serde(default)]
    pub fiber_peer_pubkey: Option<String>,
    #[serde(default)]
    pub fiber_peer_address: Option<String>,
    #[serde(default)]
    pub public_channel: Option<bool>,
    #[serde(default)]
    pub funding_udt_type_script: Option<CkbScript>,
    #[serde(default)]
    pub request_tx_hash: Option<String>,
    #[serde(default)]
    pub request_cell_out_point: Option<String>,
    #[serde(default)]
    pub signed_tx: Option<Value>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct AttachFiberPeerRequest {
    pub fiber_peer_pubkey: String,
    #[serde(default)]
    pub fiber_peer_address: Option<String>,
    #[serde(default)]
    pub public_channel: Option<bool>,
}

#[derive(Clone, Debug, Serialize)]
pub struct LiquidityQuote {
    pub asset: String,
    pub amount: u64,
    pub duration_days: u16,
    pub lease_fee: u64,
    pub routing_fee_bps: u16,
    pub available: bool,
    pub available_liquidity: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RequestIntent {
    pub id: Uuid,
    pub merchant_id: Uuid,
    pub merchant_name: String,
    pub ckb_address: String,
    pub asset: String,
    pub amount: u64,
    pub duration_days: u16,
    pub lease_fee: u64,
    pub routing_fee_bps: u16,
    #[serde(default)]
    pub fiber_peer_pubkey: Option<String>,
    #[serde(default)]
    pub fiber_peer_address: Option<String>,
    #[serde(default = "default_public_channel")]
    pub public_channel: bool,
    #[serde(default)]
    pub funding_udt_type_script: Option<CkbScript>,
    pub request_cell_id: String,
    pub memo: String,
    pub status: super::IntentStatus,
    #[serde(default)]
    pub tx_hash: Option<String>,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LiquidityRequest {
    pub id: Uuid,
    pub merchant_id: Uuid,
    pub merchant_name: String,
    #[serde(alias = "wallet_address")]
    pub ckb_address: String,
    pub asset: String,
    pub amount: u64,
    pub duration_days: u16,
    pub lease_fee: u64,
    pub routing_fee_bps: u16,
    #[serde(default)]
    pub fiber_peer_pubkey: Option<String>,
    #[serde(default)]
    pub fiber_peer_address: Option<String>,
    #[serde(default = "default_public_channel")]
    pub public_channel: bool,
    #[serde(default)]
    pub funding_udt_type_script: Option<CkbScript>,
    #[serde(default)]
    pub request_cell_id: String,
    #[serde(default)]
    pub request_tx_hash: Option<String>,
    #[serde(default)]
    pub request_cell_out_point: Option<String>,
    pub status: LiquidityStatus,
    #[serde(default)]
    pub fiber_temporary_channel_id: Option<String>,
    pub channel_id: Option<String>,
    #[serde(default)]
    pub fiber_note: Option<String>,
    #[serde(default)]
    pub fiber_error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LiquidityStatus {
    Requested,
    FundingRequired,
    FundingSubmitted,
    PendingFiberChannel,
    #[serde(alias = "deployed")]
    ChannelOpen,
    Failed,
    Expired,
    Released,
}

fn default_public_channel() -> bool {
    true
}

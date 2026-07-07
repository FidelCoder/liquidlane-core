use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use super::{LiquidityRequest, UserProfile};

#[derive(Clone, Debug, Serialize)]
pub struct Dashboard {
    pub user: UserProfile,
    pub vault: VaultSummary,
    pub deposits: Vec<Deposit>,
    pub positions: Vec<LpPosition>,
    pub liquidity_requests: Vec<LiquidityRequest>,
    pub reservations: Vec<CapacityReservation>,
    pub withdrawals: Vec<WithdrawalIntent>,
    pub fee_claims: Vec<FeeClaim>,
    pub activity: Vec<ActivityEvent>,
}

#[derive(Clone, Debug, Serialize)]
pub struct VaultSummary {
    pub asset: String,
    pub address: Option<String>,
    pub cell_out_point: Option<String>,
    pub network: String,
    pub configured: bool,
    pub scripts: VaultScripts,
    pub total_deposits: u64,
    pub reserved_liquidity: u64,
    pub pending_channel_liquidity: u64,
    pub deployed_liquidity: u64,
    pub available_liquidity: u64,
    pub fees_earned: u64,
    pub lp_count: usize,
    pub active_requests: usize,
}

#[derive(Clone, Debug, Deserialize)]
pub struct CreateDepositRequest {
    pub asset: String,
    pub amount: u64,
    pub tx_hash: Option<String>,
    #[serde(default)]
    pub intent_id: Option<Uuid>,
    #[serde(default)]
    pub signed_tx: Option<Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Deposit {
    pub id: Uuid,
    pub lp_id: Uuid,
    pub lp_name: String,
    #[serde(alias = "wallet_address")]
    pub ckb_address: String,
    pub asset: String,
    pub amount: u64,
    pub tx_hash: Option<String>,
    #[serde(default)]
    pub signed_tx: Option<Value>,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct CreateSupplyIntentRequest {
    pub asset: String,
    pub amount: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SupplyIntent {
    pub id: Uuid,
    pub lp_id: Uuid,
    pub lp_name: String,
    pub ckb_address: String,
    pub asset: String,
    pub amount: u64,
    pub vault_address: String,
    pub receipt_cell_id: String,
    pub memo: String,
    pub status: IntentStatus,
    #[serde(default)]
    pub tx_hash: Option<String>,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LpPosition {
    pub id: Uuid,
    pub lp_id: Uuid,
    pub lp_name: String,
    pub ckb_address: String,
    pub asset: String,
    pub supplied_amount: u64,
    pub available_amount: u64,
    pub reserved_amount: u64,
    pub deployed_amount: u64,
    pub fees_earned: u64,
    pub fees_claimed: u64,
    pub receipt_cell_id: String,
    #[serde(default)]
    pub receipt_cell_out_point: Option<String>,
    pub supply_tx_hash: String,
    pub status: PositionStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct CreateWithdrawalIntentRequest {
    pub position_id: Uuid,
    pub amount: u64,
}

#[derive(Clone, Debug, Deserialize)]
pub struct SettleWithdrawalRequest {
    pub tx_hash: Option<String>,
    #[serde(default)]
    pub receipt_cell_out_point: Option<String>,
    #[serde(default)]
    pub signed_tx: Option<Value>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct SettleFeeClaimRequest {
    pub tx_hash: Option<String>,
    #[serde(default)]
    pub receipt_cell_out_point: Option<String>,
    #[serde(default)]
    pub signed_tx: Option<Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WithdrawalIntent {
    pub id: Uuid,
    pub lp_id: Uuid,
    pub lp_name: String,
    pub ckb_address: String,
    pub position_id: Uuid,
    pub asset: String,
    pub amount: u64,
    pub receipt_cell_id: String,
    pub memo: String,
    pub status: IntentStatus,
    #[serde(default)]
    pub tx_hash: Option<String>,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct CreateFeeClaimRequest {
    pub position_id: Uuid,
    pub amount: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FeeClaim {
    pub id: Uuid,
    pub lp_id: Uuid,
    pub position_id: Uuid,
    pub asset: String,
    pub amount: u64,
    pub memo: String,
    pub status: IntentStatus,
    #[serde(default)]
    pub tx_hash: Option<String>,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CapacityReservation {
    pub id: Uuid,
    pub request_id: Uuid,
    pub merchant_id: Uuid,
    pub merchant_name: String,
    pub ckb_address: String,
    pub asset: String,
    pub amount: u64,
    pub lease_fee: u64,
    pub request_cell_id: String,
    pub status: ReservationStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IntentStatus {
    PendingSignature,
    Settled,
    Expired,
    Cancelled,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PositionStatus {
    Active,
    Closed,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReservationStatus {
    Reserved,
    Deployed,
    Released,
    Failed,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ActivityEvent {
    pub id: Uuid,
    pub actor_id: Uuid,
    pub label: String,
    pub amount: Option<u64>,
    pub asset: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VaultConfig {
    pub asset: String,
    pub address: Option<String>,
    pub cell_out_point: Option<String>,
    pub network: String,
    pub configured: bool,
    pub scripts: VaultScripts,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VaultScripts {
    pub vault_lock_code_hash: Option<String>,
    pub vault_lock_out_point: Option<String>,
    pub vault_type_code_hash: Option<String>,
    pub vault_type_out_point: Option<String>,
    pub lp_receipt_type_code_hash: Option<String>,
    pub lp_receipt_type_out_point: Option<String>,
    pub request_type_code_hash: Option<String>,
    pub request_type_out_point: Option<String>,
    pub fee_claim_type_code_hash: Option<String>,
    pub fee_claim_type_out_point: Option<String>,
}

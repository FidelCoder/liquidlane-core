use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize)]
pub struct VaultSummary {
    pub asset: String,
    pub total_deposits: u64,
    pub deployed_liquidity: u64,
    pub available_liquidity: u64,
    pub fees_earned: u64,
    pub lp_count: usize,
    pub active_requests: usize,
}

#[derive(Clone, Debug, Deserialize)]
pub struct CreateDepositRequest {
    pub lp_name: String,
    pub asset: String,
    pub amount: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct Deposit {
    pub id: Uuid,
    pub lp_name: String,
    pub asset: String,
    pub amount: u64,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct CreateLiquidityRequest {
    pub merchant_name: String,
    pub asset: String,
    pub amount: u64,
    pub duration_days: u16,
}

#[derive(Clone, Debug, Serialize)]
pub struct LiquidityQuote {
    pub asset: String,
    pub amount: u64,
    pub duration_days: u16,
    pub lease_fee: u64,
    pub routing_fee_bps: u16,
    pub available: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct LiquidityRequest {
    pub id: Uuid,
    pub merchant_name: String,
    pub asset: String,
    pub amount: u64,
    pub duration_days: u16,
    pub lease_fee: u64,
    pub status: LiquidityStatus,
    pub channel_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LiquidityStatus {
    Requested,
    Deployed,
}

#[derive(Clone, Debug, Serialize)]
pub struct ActivityEvent {
    pub id: Uuid,
    pub label: String,
    pub amount: Option<u64>,
    pub asset: Option<String>,
    pub created_at: DateTime<Utc>,
}

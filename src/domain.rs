use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct User {
    pub id: Uuid,
    pub display_name: String,
    #[serde(alias = "wallet_address")]
    pub ckb_address: String,
    #[serde(default = "default_wallet_type")]
    pub wallet_type: String,
    #[serde(default)]
    pub lock_script: Option<CkbScript>,
    pub role: UserRole,
    pub token: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UserRole {
    Lp,
    Merchant,
    Operator,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ChallengeRequest {
    #[serde(alias = "wallet_address")]
    pub ckb_address: String,
    #[serde(default = "default_wallet_type")]
    pub wallet_type: String,
    pub role: UserRole,
}

#[derive(Clone, Debug, Serialize)]
pub struct ChallengeResponse {
    pub challenge_id: Uuid,
    pub message: String,
    pub expires_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct VerifyWalletRequest {
    pub challenge_id: Uuid,
    #[serde(alias = "wallet_address")]
    pub ckb_address: String,
    #[serde(default = "default_wallet_type")]
    pub wallet_type: String,
    pub signature: String,
    #[serde(default)]
    pub lock_script: Option<CkbScript>,
    pub display_name: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct AuthResponse {
    pub token: String,
    pub user: UserProfile,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuthChallenge {
    pub id: Uuid,
    #[serde(alias = "wallet_address")]
    pub ckb_address: String,
    #[serde(default = "default_wallet_type")]
    pub wallet_type: String,
    pub role: UserRole,
    pub message: String,
    pub expires_at: DateTime<Utc>,
    pub consumed: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct UserProfile {
    pub id: Uuid,
    pub display_name: String,
    pub ckb_address: String,
    pub wallet_type: String,
    pub role: UserRole,
}

impl From<&User> for UserProfile {
    fn from(user: &User) -> Self {
        Self {
            id: user.id,
            display_name: user.display_name.clone(),
            ckb_address: user.ckb_address.clone(),
            wallet_type: user.wallet_type.clone(),
            role: user.role.clone(),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Dashboard {
    pub user: UserProfile,
    pub vault: VaultSummary,
    pub deposits: Vec<Deposit>,
    pub liquidity_requests: Vec<LiquidityRequest>,
    pub activity: Vec<ActivityEvent>,
}

#[derive(Clone, Debug, Serialize)]
pub struct VaultSummary {
    pub asset: String,
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
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct CreateLiquidityRequest {
    pub asset: String,
    pub amount: u64,
    pub duration_days: u16,
    #[serde(default)]
    pub fiber_peer_pubkey: Option<String>,
    #[serde(default)]
    pub public_channel: Option<bool>,
    #[serde(default)]
    pub funding_udt_type_script: Option<CkbScript>,
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
    #[serde(default = "default_public_channel")]
    pub public_channel: bool,
    #[serde(default)]
    pub funding_udt_type_script: Option<CkbScript>,
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
    PendingFiberChannel,
    #[serde(alias = "deployed")]
    ChannelOpen,
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
pub struct CkbScript {
    pub code_hash: String,
    pub hash_type: String,
    pub args: String,
}

fn default_wallet_type() -> String {
    "ckb".to_string()
}

fn default_public_channel() -> bool {
    true
}

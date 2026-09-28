use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const SHANNONS: u64 = 100_000_000;
pub const TESTNET_GENESIS: &str =
    "0x10639e0895502b5688a6be8cf69460d76541bfa4821629d86d62ba0aae3f9606";
pub const PROTOCOL: &str = "liquidlane-marketplace/1";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Account {
    pub address: String,
    pub created_at: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Node {
    pub id: String,
    pub owner: String,
    pub role: String,
    pub label: String,
    pub pubkey: String,
    pub address: String,
    pub version: String,
    pub last_seen: i64,
    pub available_ckb: u64,
    pub reserve_ckb: u64,
    #[serde(default)]
    pub provider_policy: Option<ProviderPolicy>,
    #[serde(default)]
    pub funding: Option<NodeFunding>,
    #[serde(default)]
    pub background: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NodeFunding {
    pub address: String,
    pub payload: String,
    pub signature: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProviderPolicy {
    pub accept_public_orders: bool,
    pub auto_approve: bool,
    pub max_order_ckb: u64,
    pub max_total_ckb: u64,
    pub min_fee_ckb: u64,
    pub committed_ckb: u64,
}

impl Node {
    pub fn automatic(&self) -> bool {
        self.provider_policy
            .as_ref()
            .is_some_and(|p| p.accept_public_orders && p.auto_approve)
    }
    pub fn available_funding(&self, reserved: u64) -> u64 {
        let wallet = self
            .available_ckb
            .saturating_sub(63)
            .saturating_sub(reserved);
        self.provider_policy.as_ref().map_or(wallet, |p| {
            wallet.min(p.max_order_ckb).min(
                p.max_total_ckb
                    .saturating_sub(p.committed_ckb)
                    .saturating_sub(reserved),
            )
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Offer {
    pub id: String,
    pub provider_node: String,
    pub owner: String,
    pub min_capacity_ckb: u64,
    pub max_capacity_ckb: u64,
    pub opening_fee_ckb: u64,
    pub public_channel: bool,
    pub enabled: bool,
    pub created_at: i64,
    #[serde(default)]
    pub expires_at: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Quote {
    pub protocol: String,
    pub network: String,
    pub order_id: String,
    pub provider_node: String,
    pub provider_pubkey: String,
    pub merchant_node: String,
    pub merchant_pubkey: String,
    pub merchant_address: String,
    pub merchant_account: String,
    pub capacity_ckb: u64,
    pub funding_ckb: u64,
    pub provider_reserve_ckb: u64,
    pub merchant_reserve_ckb: u64,
    pub opening_fee_ckb: u64,
    pub fee_recipient: String,
    pub public_channel: bool,
    pub expires_at: i64,
    pub nonce: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Order {
    pub id: String,
    pub owner: String,
    pub offer_id: String,
    #[serde(default)]
    pub automatic: bool,
    pub quote: Quote,
    pub quote_hash: String,
    pub provider_signature: Option<String>,
    pub merchant_proof: Option<Value>,
    pub status: String,
    pub channel_id: Option<String>,
    pub funding_outpoint: Option<String>,
    pub provider_evidence: Option<ChannelEvidence>,
    pub merchant_evidence: Option<ChannelEvidence>,
    pub probe_invoice: Option<String>,
    pub probe_payment_hash: Option<String>,
    pub probe_received: bool,
    #[serde(default)]
    pub delivery_block_number: Option<u64>,
    #[serde(default)]
    pub delivered_at: Option<i64>,
    pub fee_status: String,
    pub fee_tx_hash: Option<String>,
    pub error: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl Order {
    pub fn reserves_funds(&self) -> bool {
        matches!(
            self.status.as_str(),
            "awaiting_quote"
                | "quoted"
                | "accepted"
                | "approved"
                | "opening"
                | "awaiting_confirmation"
                | "verifying"
                | "reconciling"
        )
    }
    pub fn approval_message(&self) -> String {
        format!(
            "LiquidLane capacity order approval\nProtocol: {PROTOCOL}\nNetwork: testnet\nOrder: {}\nQuote SHA-256: {}\nThis authorizes only the quoted channel opening and its stated service fee.",
            self.id, self.quote_hash
        )
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChannelEvidence {
    pub channel_id: String,
    pub peer_pubkey: String,
    pub funding_outpoint: String,
    pub state: String,
    pub local_balance: String,
    pub remote_balance: String,
    pub inbound_liquidity: String,
    pub observed_at: i64,
    pub settlement_tx_hash: Option<String>,
    #[serde(default)]
    pub state_flags: Option<String>,
    #[serde(default)]
    pub settlement: Option<super::settlement::Settlement>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Event {
    pub order_id: String,
    pub actor: String,
    pub action: String,
    pub at: i64,
}

pub fn now() -> i64 {
    chrono::Utc::now().timestamp()
}
pub fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}
pub fn token() -> String {
    format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}

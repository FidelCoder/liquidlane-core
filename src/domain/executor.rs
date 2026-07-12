use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExecutorJob {
    pub id: Uuid,
    pub request_id: Uuid,
    pub status: ExecutorJobStatus,
    pub attempts: u8,
    pub max_retries: u8,
    #[serde(default)]
    pub last_error: Option<String>,
    #[serde(default)]
    pub fiber_ref: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExecutorJobStatus {
    Queued,
    Preparing,
    Submitted,
    AwaitingVaultFunding,
    AwaitingFundingConfirmation,
    ChannelActive,
    RetryableFailed,
    TerminalFailed,
}

impl ExecutorJobStatus {
    pub fn is_open(&self) -> bool {
        matches!(
            self,
            Self::Queued
                | Self::Preparing
                | Self::Submitted
                | Self::AwaitingVaultFunding
                | Self::AwaitingFundingConfirmation
                | Self::RetryableFailed
        )
    }
}

pub const FUNDING_MODE_VAULT_EXTERNAL: &str = "vault_external";
pub const FUNDING_MODE_NODE_WALLET_DIAGNOSTIC: &str = "node_wallet_diagnostic";
pub const FUNDING_MODE_LEGACY_MANAGED_NODE_BETA: &str = "managed_node_beta";

pub fn normalize_executor_funding_mode(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        FUNDING_MODE_NODE_WALLET_DIAGNOSTIC | FUNDING_MODE_LEGACY_MANAGED_NODE_BETA => {
            FUNDING_MODE_NODE_WALLET_DIAGNOSTIC.to_string()
        }
        _ => FUNDING_MODE_VAULT_EXTERNAL.to_string(),
    }
}

pub fn is_vault_external_funding_mode(value: &str) -> bool {
    normalize_executor_funding_mode(value) == FUNDING_MODE_VAULT_EXTERNAL
}

pub fn is_node_wallet_diagnostic_mode(value: &str) -> bool {
    normalize_executor_funding_mode(value) == FUNDING_MODE_NODE_WALLET_DIAGNOSTIC
}

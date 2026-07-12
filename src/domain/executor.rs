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
                | Self::AwaitingFundingConfirmation
                | Self::RetryableFailed
        )
    }
}

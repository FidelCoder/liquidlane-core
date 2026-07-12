use serde::Serialize;

use super::AppStore;
use crate::domain::LiquidityStatus;

#[derive(Clone, Debug, Serialize)]
pub struct ExecutorHealth {
    pub enabled: bool,
    pub fiber_rpc_configured: bool,
    pub funding_mode: String,
    pub poll_interval_ms: u64,
    pub max_retries: u8,
    pub queued_requests: usize,
    pub pending_handoffs: usize,
    pub failed_requests: usize,
    pub external_funding_supported: bool,
}

impl AppStore {
    pub fn executor_enabled(&self) -> bool {
        self.executor_enabled
    }

    pub async fn executor_health(&self) -> ExecutorHealth {
        let state = self.inner.read().await;
        let queued_requests = state
            .liquidity_requests
            .iter()
            .filter(|request| {
                request.status == LiquidityStatus::Requested
                    && request
                        .fiber_peer_pubkey
                        .as_deref()
                        .is_some_and(|value| !value.is_empty())
            })
            .count();
        let pending_handoffs = state
            .liquidity_requests
            .iter()
            .filter(|request| request.status == LiquidityStatus::PendingFiberChannel)
            .count();
        let failed_requests = state
            .liquidity_requests
            .iter()
            .filter(|request| request.status == LiquidityStatus::Failed)
            .count();

        ExecutorHealth {
            enabled: self.executor_enabled,
            fiber_rpc_configured: self.fiber.is_configured(),
            funding_mode: self.executor_funding_mode.clone(),
            poll_interval_ms: self.executor_poll_interval_ms,
            max_retries: self.executor_max_retries,
            queued_requests,
            pending_handoffs,
            failed_requests,
            external_funding_supported: true,
        }
    }
}

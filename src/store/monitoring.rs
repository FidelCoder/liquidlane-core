use chrono::{DateTime, Utc};
use serde::Serialize;

use super::AppStore;
use crate::domain::{ExecutorJobStatus, LiquidityStatus, PositionStatus};

#[derive(Clone, Debug, Serialize)]
pub struct CoreStateExport {
    pub users: usize,
    pub lp_positions: usize,
    pub active_lp_positions: usize,
    pub total_supplied: u64,
    pub total_available: u64,
    pub total_reserved: u64,
    pub total_deployed: u64,
    pub liquidity_requests: usize,
    pub active_requests: usize,
    pub pending_fiber_requests: usize,
    pub open_channels: usize,
    pub failed_requests: usize,
    pub released_requests: usize,
    pub executor_jobs: usize,
    pub open_executor_jobs: usize,
    pub activity_events: usize,
    pub last_event_at: Option<DateTime<Utc>>,
}

impl AppStore {
    pub async fn state_export_summary(&self) -> CoreStateExport {
        let state = self.inner.read().await;
        let active_positions = state
            .lp_positions
            .iter()
            .filter(|position| position.status == PositionStatus::Active);
        let total_supplied = active_positions
            .clone()
            .map(|position| position.supplied_amount)
            .sum();
        let total_available = active_positions
            .clone()
            .map(|position| position.available_amount)
            .sum();
        let total_reserved = active_positions
            .clone()
            .map(|position| position.reserved_amount)
            .sum();
        let total_deployed = active_positions
            .clone()
            .map(|position| position.deployed_amount)
            .sum();
        let active_requests = state
            .liquidity_requests
            .iter()
            .filter(|request| {
                matches!(
                    request.status,
                    LiquidityStatus::Requested | LiquidityStatus::PendingFiberChannel
                )
            })
            .count();
        let pending_fiber_requests = state
            .liquidity_requests
            .iter()
            .filter(|request| request.status == LiquidityStatus::PendingFiberChannel)
            .count();
        let open_channels = state
            .liquidity_requests
            .iter()
            .filter(|request| request.status == LiquidityStatus::ChannelOpen)
            .count();
        let failed_requests = state
            .liquidity_requests
            .iter()
            .filter(|request| request.status == LiquidityStatus::Failed)
            .count();
        let released_requests = state
            .liquidity_requests
            .iter()
            .filter(|request| {
                matches!(
                    request.status,
                    LiquidityStatus::Released | LiquidityStatus::Expired
                )
            })
            .count();
        let open_executor_jobs = state
            .executor_jobs
            .iter()
            .filter(|job| job.status.is_open())
            .count();
        let _terminal_jobs = state
            .executor_jobs
            .iter()
            .filter(|job| job.status == ExecutorJobStatus::TerminalFailed)
            .count();

        CoreStateExport {
            users: state.users.len(),
            lp_positions: state.lp_positions.len(),
            active_lp_positions: state
                .lp_positions
                .iter()
                .filter(|position| position.status == PositionStatus::Active)
                .count(),
            total_supplied,
            total_available,
            total_reserved,
            total_deployed,
            liquidity_requests: state.liquidity_requests.len(),
            active_requests,
            pending_fiber_requests,
            open_channels,
            failed_requests,
            released_requests,
            executor_jobs: state.executor_jobs.len(),
            open_executor_jobs,
            activity_events: state.events.len(),
            last_event_at: state.events.first().map(|event| event.created_at),
        }
    }
}

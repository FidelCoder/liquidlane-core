use chrono::Utc;

use super::AppStore;
use crate::domain::{ExternalFundingWatcherState, LiquidityStatus};

impl AppStore {
    pub async fn external_funding_watcher_state(&self) -> ExternalFundingWatcherState {
        let state = self.inner.read().await;
        let release_candidates = state
            .liquidity_requests
            .iter()
            .filter(|request| {
                matches!(
                    request.status,
                    LiquidityStatus::FundingRequired | LiquidityStatus::Failed
                ) && request.created_at + chrono::Duration::days(i64::from(request.duration_days))
                    <= Utc::now()
            })
            .count();
        ExternalFundingWatcherState {
            funding_required: count_status(&state, LiquidityStatus::FundingRequired),
            funding_submitted: count_status(&state, LiquidityStatus::FundingSubmitted),
            pending_fiber_channel: count_status(&state, LiquidityStatus::PendingFiberChannel),
            channel_open: count_status(&state, LiquidityStatus::ChannelOpen),
            failed: count_status(&state, LiquidityStatus::Failed),
            released_or_settled: state
                .liquidity_requests
                .iter()
                .filter(|request| {
                    matches!(
                        request.status,
                        LiquidityStatus::Released | LiquidityStatus::Settled
                    )
                })
                .count(),
            release_candidates,
            open_jobs: state
                .executor_jobs
                .iter()
                .filter(|job| job.status.is_open())
                .count(),
            last_event_at: state.events.first().map(|event| event.created_at),
        }
    }
}

fn count_status(state: &super::StoreState, status: LiquidityStatus) -> usize {
    state
        .liquidity_requests
        .iter()
        .filter(|request| request.status == status)
        .count()
}

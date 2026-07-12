use anyhow::{Result, anyhow};
use chrono::{Duration, Utc};
use serde::Serialize;
use uuid::Uuid;

use super::{AppStore, accounting::release_positions};
use crate::domain::{
    ActivityEvent, ExecutorJob, ExecutorJobStatus, ExternalFundingReadiness, LiquidityRequest,
    LiquidityStatus, ReservationStatus, is_node_wallet_diagnostic_mode,
    is_vault_external_funding_mode,
};

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
    pub open_jobs: usize,
    pub external_funding_supported: bool,
    pub external_funding_ready: bool,
    pub external_funding_blockers: Vec<String>,
    pub external_funding: ExternalFundingReadiness,
    pub vault_external_required: bool,
    pub node_wallet_diagnostic: bool,
}

impl AppStore {
    pub fn executor_enabled(&self) -> bool {
        self.executor_enabled
    }

    pub async fn executor_health(&self) -> ExecutorHealth {
        let state = self.inner.read().await;
        let queued_requests = state
            .executor_jobs
            .iter()
            .filter(|job| job.status == ExecutorJobStatus::Queued)
            .count();
        let open_jobs = state
            .executor_jobs
            .iter()
            .filter(|job| job.status.is_open())
            .count();
        let pending_handoffs = state
            .liquidity_requests
            .iter()
            .filter(|request| {
                matches!(
                    request.status,
                    LiquidityStatus::FundingRequired
                        | LiquidityStatus::FundingSubmitted
                        | LiquidityStatus::PendingFiberChannel
                )
            })
            .count();
        let failed_requests = state
            .liquidity_requests
            .iter()
            .filter(|request| request.status == LiquidityStatus::Failed)
            .count();

        let vault_external_required = is_vault_external_funding_mode(&self.executor_funding_mode);
        let node_wallet_diagnostic = is_node_wallet_diagnostic_mode(&self.executor_funding_mode);
        drop(state);
        let external_funding = self.external_funding_readiness().await;

        ExecutorHealth {
            enabled: self.executor_enabled,
            fiber_rpc_configured: self.fiber.is_configured(),
            funding_mode: self.executor_funding_mode.clone(),
            poll_interval_ms: self.executor_poll_interval_ms,
            max_retries: self.executor_max_retries,
            queued_requests,
            pending_handoffs,
            failed_requests,
            open_jobs,
            external_funding_supported: external_funding.supported,
            external_funding_ready: external_funding.ready,
            external_funding_blockers: external_funding.blockers.clone(),
            external_funding,
            vault_external_required,
            node_wallet_diagnostic,
        }
    }

    pub async fn executor_jobs(&self) -> Vec<ExecutorJob> {
        let mut jobs = self.inner.read().await.executor_jobs.clone();
        jobs.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        jobs
    }

    pub(super) async fn ensure_executor_job(
        &self,
        request: &LiquidityRequest,
    ) -> Result<ExecutorJob> {
        let mut state = self.inner.write().await;
        if let Some(job) = state
            .executor_jobs
            .iter()
            .find(|job| job.request_id == request.id)
        {
            return Ok(job.clone());
        }
        let now = Utc::now();
        let job = ExecutorJob {
            id: Uuid::new_v4(),
            request_id: request.id,
            status: ExecutorJobStatus::Queued,
            attempts: 0,
            max_retries: self.executor_max_retries,
            last_error: None,
            fiber_ref: None,
            created_at: now,
            updated_at: now,
        };
        state.executor_jobs.push(job.clone());
        self.persist_locked(&state).await?;
        Ok(job)
    }

    pub(super) async fn mark_executor_job(
        &self,
        request_id: Uuid,
        status: ExecutorJobStatus,
        error: Option<String>,
        fiber_ref: Option<String>,
    ) -> Result<()> {
        let mut state = self.inner.write().await;
        let Some(job) = state
            .executor_jobs
            .iter_mut()
            .find(|job| job.request_id == request_id)
        else {
            return Ok(());
        };
        if status == ExecutorJobStatus::Preparing {
            job.attempts = job.attempts.saturating_add(1);
        }
        job.status =
            if status == ExecutorJobStatus::RetryableFailed && job.attempts >= job.max_retries {
                ExecutorJobStatus::TerminalFailed
            } else {
                status
            };
        job.last_error = error;
        if fiber_ref.is_some() {
            job.fiber_ref = fiber_ref;
        }
        job.updated_at = Utc::now();
        self.persist_locked(&state).await
    }

    pub async fn retry_executor_job(&self, job_id: Uuid) -> Result<ExecutorJob> {
        let request_id = {
            let state = self.inner.read().await;
            let job = state
                .executor_jobs
                .iter()
                .find(|job| job.id == job_id)
                .ok_or_else(|| anyhow!("executor job not found"))?;
            if job.status == ExecutorJobStatus::TerminalFailed {
                return Err(anyhow!(
                    "executor job reached max retries; create a new request or repair manually"
                ));
            }
            job.request_id
        };
        self.mark_executor_job(request_id, ExecutorJobStatus::Queued, None, None)
            .await?;
        let _ = self.try_execute_liquidity_request(request_id).await;
        self.executor_jobs()
            .await
            .into_iter()
            .find(|job| job.id == job_id)
            .ok_or_else(|| anyhow!("executor job not found after retry"))
    }

    pub async fn release_expired_requests(&self) -> Result<usize> {
        let now = Utc::now();
        let mut state = self.inner.write().await;
        let mut released = 0usize;
        let mut events = Vec::new();
        for request in state.liquidity_requests.iter_mut() {
            if !matches!(
                request.status,
                LiquidityStatus::Requested
                    | LiquidityStatus::FundingRequired
                    | LiquidityStatus::Failed
            ) {
                continue;
            }
            if request.created_at + Duration::days(i64::from(request.duration_days)) > now {
                continue;
            }
            request.status = LiquidityStatus::Released;
            request.fiber_note =
                Some("Reservation expired; liquidity returned to LP availability.".to_string());
            request.updated_at = now;
            released += 1;
            events.push((
                request.merchant_id,
                request.merchant_name.clone(),
                request.amount,
                request.asset.clone(),
                request.id,
            ));
        }
        for (_, _, amount, asset, request_id) in &events {
            release_positions(&mut state.lp_positions, asset, *amount, now)?;
            if let Some(reservation) = state
                .capacity_reservations
                .iter_mut()
                .find(|item| item.request_id == *request_id)
            {
                reservation.status = ReservationStatus::Released;
                reservation.updated_at = now;
            }
        }
        for (actor_id, merchant_name, amount, asset, _) in events {
            state.events.insert(
                0,
                ActivityEvent {
                    id: Uuid::new_v4(),
                    actor_id,
                    label: format!("Expired receive-capacity request released for {merchant_name}"),
                    amount: Some(amount),
                    asset: Some(asset),
                    created_at: now,
                },
            );
        }
        if released > 0 {
            self.persist_locked(&state).await?;
        }
        Ok(released)
    }
}

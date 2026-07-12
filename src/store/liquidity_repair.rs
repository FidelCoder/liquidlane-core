use anyhow::{Result, anyhow};
use chrono::Utc;
use uuid::Uuid;

use super::{
    AppStore,
    accounting::{release_positions, settle_positions},
};
use crate::domain::{
    ActivityEvent, ExecutorJobStatus, LiquidityRequest, LiquidityStatus, ReleaseLiquidityRequest,
    ReservationStatus, SettleLiquidityRequest,
};

impl AppStore {
    pub async fn release_liquidity_request(
        &self,
        id: Uuid,
        request: ReleaseLiquidityRequest,
    ) -> Result<LiquidityRequest> {
        if let Some(tx_hash) = request.tx_hash.as_deref() {
            self.verify_ckb_settlement_tx(tx_hash, &request.signed_tx)
                .await?;
        }
        let now = Utc::now();
        let mut state = self.inner.write().await;
        let updated = {
            let stored = request_mut(&mut state, id)?;
            if !matches!(
                stored.status,
                LiquidityStatus::Requested
                    | LiquidityStatus::FundingRequired
                    | LiquidityStatus::FundingSubmitted
                    | LiquidityStatus::PendingFiberChannel
                    | LiquidityStatus::Failed
                    | LiquidityStatus::Expired
            ) {
                return Err(anyhow!(
                    "only reserved, failed, or expired requests can be released"
                ));
            }
            stored.status = LiquidityStatus::Released;
            stored.fiber_error = None;
            stored.fiber_note = Some(release_note(&request));
            stored.updated_at = now;
            stored.clone()
        };
        release_positions(&mut state.lp_positions, &updated.asset, updated.amount, now)?;
        mark_reservation(&mut state, updated.id, ReservationStatus::Released, now);
        mark_job_released(&mut state, updated.id, now);
        state.events.insert(
            0,
            ActivityEvent {
                id: Uuid::new_v4(),
                actor_id: updated.merchant_id,
                label: format!("Released reserved liquidity for {}", updated.merchant_name),
                amount: Some(updated.amount),
                asset: Some(updated.asset.clone()),
                created_at: now,
            },
        );
        self.persist_locked(&state).await?;
        Ok(updated)
    }

    pub async fn settle_liquidity_request(
        &self,
        id: Uuid,
        request: SettleLiquidityRequest,
    ) -> Result<LiquidityRequest> {
        if let Some(tx_hash) = request.tx_hash.as_deref() {
            self.verify_ckb_settlement_tx(tx_hash, &request.signed_tx)
                .await?;
        }
        let now = Utc::now();
        let mut state = self.inner.write().await;
        let updated = {
            let stored = request_mut(&mut state, id)?;
            if stored.status != LiquidityStatus::ChannelOpen {
                return Err(anyhow!("only active channel requests can be settled"));
            }
            if let Some(channel_id) = request.channel_id.filter(|value| !value.trim().is_empty()) {
                stored.channel_id = Some(channel_id);
            }
            stored.status = LiquidityStatus::Settled;
            stored.fiber_error = None;
            stored.fiber_note = Some(
                "Fiber channel settled; LP liquidity returned to available balance.".to_string(),
            );
            stored.updated_at = now;
            stored.clone()
        };
        settle_positions(&mut state.lp_positions, &updated.asset, updated.amount, now)?;
        mark_reservation(&mut state, updated.id, ReservationStatus::Released, now);
        mark_job_settled(&mut state, updated.id, now);
        state.events.insert(
            0,
            ActivityEvent {
                id: Uuid::new_v4(),
                actor_id: updated.merchant_id,
                label: format!("Settled Fiber channel for {}", updated.merchant_name),
                amount: Some(updated.amount),
                asset: Some(updated.asset.clone()),
                created_at: now,
            },
        );
        self.persist_locked(&state).await?;
        Ok(updated)
    }
}

fn request_mut(state: &mut super::StoreState, id: Uuid) -> Result<&mut LiquidityRequest> {
    state
        .liquidity_requests
        .iter_mut()
        .find(|request| request.id == id)
        .ok_or_else(|| anyhow!("liquidity request not found"))
}

fn mark_reservation(
    state: &mut super::StoreState,
    request_id: Uuid,
    status: ReservationStatus,
    now: chrono::DateTime<Utc>,
) {
    if let Some(reservation) = state
        .capacity_reservations
        .iter_mut()
        .find(|item| item.request_id == request_id)
    {
        reservation.status = status;
        reservation.updated_at = now;
    }
}

fn mark_job_released(state: &mut super::StoreState, request_id: Uuid, now: chrono::DateTime<Utc>) {
    mark_job(state, request_id, ExecutorJobStatus::Released, now);
}

fn mark_job_settled(state: &mut super::StoreState, request_id: Uuid, now: chrono::DateTime<Utc>) {
    mark_job(state, request_id, ExecutorJobStatus::ChannelSettled, now);
}

fn mark_job(
    state: &mut super::StoreState,
    request_id: Uuid,
    status: ExecutorJobStatus,
    now: chrono::DateTime<Utc>,
) {
    if let Some(job) = state
        .executor_jobs
        .iter_mut()
        .find(|job| job.request_id == request_id)
    {
        job.status = status;
        job.updated_at = now;
    }
}

fn release_note(request: &ReleaseLiquidityRequest) -> String {
    let base = if request.tx_hash.is_some() {
        "Release transaction verified; reserved liquidity returned to LP availability."
    } else {
        "Reserved liquidity released by internal repair; on-chain release tx builder is still pending."
    };
    request
        .reason
        .as_deref()
        .filter(|reason| !reason.trim().is_empty())
        .map(|reason| format!("{base} Reason: {reason}"))
        .unwrap_or_else(|| base.to_string())
}

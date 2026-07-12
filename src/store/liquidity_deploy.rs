use anyhow::{Result, anyhow};
use chrono::Utc;
use uuid::Uuid;

use super::{
    AppStore,
    accounting::{deploy_positions, undeploy_positions},
    validation::require_role,
};
use crate::domain::{
    ActivityEvent, ExecutorJobStatus, LiquidityRequest, LiquidityStatus, ReservationStatus, User,
    UserRole, is_vault_external_funding_mode,
};

impl AppStore {
    pub async fn deploy_liquidity(&self, user: &User, id: Uuid) -> Result<LiquidityRequest> {
        require_role(user, &[UserRole::Merchant, UserRole::Operator])?;
        self.authorized_liquidity_request(user, id).await?;
        self.submit_fiber_handoff(id, user.id, false).await
    }

    pub async fn try_execute_liquidity_request(&self, id: Uuid) -> Option<LiquidityRequest> {
        if !self.executor_enabled() {
            return None;
        }
        match self.stored_liquidity_request(id).await {
            Ok(request) => {
                let _ = self.ensure_executor_job(&request).await;
            }
            Err(error) => {
                tracing::warn!(request_id = %id, error = %error, "LiquidLane executor could not load request");
                return None;
            }
        }
        match self.submit_fiber_handoff(id, Uuid::nil(), true).await {
            Ok(request) => Some(request),
            Err(error) => {
                tracing::warn!(request_id = %id, error = %error, "LiquidLane executor handoff failed");
                None
            }
        }
    }

    pub(super) async fn submit_fiber_handoff(
        &self,
        id: Uuid,
        actor_id: Uuid,
        executor: bool,
    ) -> Result<LiquidityRequest> {
        let request = self.stored_liquidity_request(id).await?;
        if matches!(
            request.status,
            LiquidityStatus::PendingFiberChannel | LiquidityStatus::ChannelOpen
        ) {
            return Ok(request);
        }
        if request.request_tx_hash.is_none() {
            let note = "LiquidLane executor is waiting for the on-chain capacity request before Fiber handoff.";
            if executor {
                let _ = self
                    .mark_executor_job(id, ExecutorJobStatus::Queued, None, None)
                    .await;
                return self.mark_executor_note(id, note).await;
            }
            return Err(anyhow!(note));
        }
        if request
            .fiber_peer_pubkey
            .as_deref()
            .unwrap_or_default()
            .is_empty()
        {
            let note = "Merchant Fiber receive node is required before LiquidLane can execute the channel handoff.";
            if executor {
                let _ = self
                    .mark_executor_job(id, ExecutorJobStatus::Queued, None, None)
                    .await;
                return self.mark_executor_note(id, note).await;
            }
            return Err(anyhow!(note));
        }
        if !self.fiber.is_configured() {
            let note = "LiquidLane executor queued this request; Fiber RPC is not configured yet.";
            if executor {
                let _ = self
                    .mark_executor_job(
                        id,
                        ExecutorJobStatus::RetryableFailed,
                        Some(note.to_string()),
                        None,
                    )
                    .await;
                return self.mark_executor_note(id, note).await;
            }
            return Err(anyhow!(
                "FIBER_RPC_URL is required before submitting Fiber open_channel"
            ));
        }

        if executor {
            let _ = self
                .mark_executor_job(id, ExecutorJobStatus::Preparing, None, None)
                .await;
        }
        if is_vault_external_funding_mode(&self.executor_funding_mode) {
            let updated = self
                .prepare_external_funding_intent(&request, actor_id, executor)
                .await?;
            if executor {
                let _ = self
                    .mark_executor_job(
                        id,
                        ExecutorJobStatus::AwaitingVaultFunding,
                        updated.fiber_error.clone(),
                        updated.fiber_temporary_channel_id.clone(),
                    )
                    .await;
            }
            return Ok(updated);
        }
        let outcome = self.fiber.open_channel(&request).await;
        let mut state = self.inner.write().await;
        let request = state
            .liquidity_requests
            .iter_mut()
            .find(|request| request.id == id)
            .ok_or_else(|| anyhow!("liquidity request not found"))?;

        let now = Utc::now();
        let job_status;
        let mut job_error = None;
        let mut job_ref = None;
        let event_label = match outcome {
            Ok(outcome) => {
                request.status = if outcome.channel_id.is_some() {
                    LiquidityStatus::ChannelOpen
                } else {
                    LiquidityStatus::PendingFiberChannel
                };
                request.fiber_temporary_channel_id = outcome.temporary_channel_id;
                request.channel_id = outcome.channel_id;
                request.fiber_note = outcome.note;
                request.fiber_error = None;
                request.updated_at = now;
                job_ref = request
                    .channel_id
                    .clone()
                    .or_else(|| request.fiber_temporary_channel_id.clone());
                job_status = if request.status == LiquidityStatus::ChannelOpen {
                    ExecutorJobStatus::ChannelActive
                } else {
                    ExecutorJobStatus::AwaitingFundingConfirmation
                };
                if executor {
                    format!(
                        "LiquidLane executor submitted Fiber handoff for {}",
                        request.merchant_name
                    )
                } else {
                    format!("Submitted Fiber handoff for {}", request.merchant_name)
                }
            }
            Err(error) => {
                request.status = LiquidityStatus::Failed;
                let message = error.to_string();
                request.fiber_error = Some(message.clone());
                request.fiber_note =
                    Some(fiber_failure_note(&self.executor_funding_mode).to_string());
                request.updated_at = now;
                job_status = ExecutorJobStatus::RetryableFailed;
                job_error = Some(message);
                if executor {
                    format!(
                        "LiquidLane executor Fiber handoff failed for {}",
                        request.merchant_name
                    )
                } else {
                    format!("Fiber handoff failed for {}", request.merchant_name)
                }
            }
        };

        let updated = request.clone();
        update_reservation_and_positions(&mut state, &updated, now);
        let event_actor_id = if executor {
            updated.merchant_id
        } else {
            actor_id
        };
        state.events.insert(
            0,
            ActivityEvent {
                id: Uuid::new_v4(),
                actor_id: event_actor_id,
                label: event_label,
                amount: Some(updated.amount),
                asset: Some(updated.asset.clone()),
                created_at: now,
            },
        );
        self.persist_locked(&state).await?;
        drop(state);
        if executor {
            let _ = self
                .mark_executor_job(id, job_status, job_error, job_ref)
                .await;
        }

        Ok(updated)
    }
}

pub(super) fn update_reservation_and_positions(
    state: &mut super::StoreState,
    updated: &LiquidityRequest,
    now: chrono::DateTime<Utc>,
) {
    if let Some(reservation) = state
        .capacity_reservations
        .iter_mut()
        .find(|reservation| reservation.request_id == updated.id)
    {
        reservation.updated_at = now;
        match updated.status {
            LiquidityStatus::FundingRequired
            | LiquidityStatus::FundingSubmitted
            | LiquidityStatus::PendingFiberChannel => {
                if reservation.status == ReservationStatus::Failed {
                    reservation.status = ReservationStatus::Reserved;
                }
            }
            LiquidityStatus::ChannelOpen => {
                if reservation.status != ReservationStatus::Deployed {
                    if let Err(error) = deploy_positions(
                        &mut state.lp_positions,
                        &updated.asset,
                        updated.amount,
                        now,
                    ) {
                        tracing::warn!(request_id = %updated.id, error = %error, "failed to mark reserved LP liquidity as deployed");
                    }
                }
                reservation.status = ReservationStatus::Deployed;
            }
            LiquidityStatus::Failed => {
                if reservation.status == ReservationStatus::Deployed {
                    if let Err(error) = undeploy_positions(
                        &mut state.lp_positions,
                        &updated.asset,
                        updated.amount,
                        now,
                    ) {
                        tracing::warn!(request_id = %updated.id, error = %error, "failed to return deployed LP liquidity to reserved");
                    }
                    reservation.status = ReservationStatus::Reserved;
                } else if reservation.status == ReservationStatus::Released {
                    reservation.status = ReservationStatus::Failed;
                }
            }
            LiquidityStatus::Expired | LiquidityStatus::Released | LiquidityStatus::Settled => {
                reservation.status = ReservationStatus::Released;
            }
            LiquidityStatus::Requested => {}
        }
    }
}

fn fiber_failure_note(funding_mode: &str) -> &'static str {
    if is_vault_external_funding_mode(funding_mode) {
        return "The merchant reserve is confirmed on-chain, but vault-funded Fiber execution is waiting for the external funding transaction builder.";
    }
    "LiquidLane kept the reserve visible so the diagnostic handoff can be repaired."
}

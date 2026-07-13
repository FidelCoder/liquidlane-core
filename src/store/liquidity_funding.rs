use anyhow::Result;
use chrono::Utc;
use uuid::Uuid;

use super::{AppStore, liquidity_deploy::update_reservation_and_positions};
use crate::domain::{
    ActivityEvent, ExecutorJobStatus, ExternalFundingIntentStatus, LiquidityRequest,
    LiquidityStatus,
};

impl AppStore {
    pub(super) async fn execute_vault_funded_handoff(
        &self,
        request: &LiquidityRequest,
        actor_id: Uuid,
        executor: bool,
    ) -> Result<LiquidityRequest> {
        let readiness = self.external_funding_readiness().await;
        if !readiness.ready {
            let updated = self
                .prepare_external_funding_intent(request, actor_id, executor)
                .await?;
            if executor {
                let _ = self
                    .mark_executor_job(
                        request.id,
                        ExecutorJobStatus::AwaitingVaultFunding,
                        updated.fiber_error.clone(),
                        updated.fiber_temporary_channel_id.clone(),
                    )
                    .await;
            }
            return Ok(updated);
        }

        self.mark_vault_funding_negotiating(request, actor_id, executor)
            .await?;
        let outcome = self.fiber.open_channel(request).await;
        let mut state = self.inner.write().await;
        let now = Utc::now();
        let stored = state
            .liquidity_requests
            .iter_mut()
            .find(|stored| stored.id == request.id)
            .ok_or_else(|| anyhow::anyhow!("liquidity request not found"))?;

        let (job_status, job_error, job_ref, label) = match outcome {
            Ok(outcome) => {
                stored.status = if outcome.channel_id.is_some() {
                    LiquidityStatus::ChannelOpen
                } else {
                    LiquidityStatus::PendingFiberChannel
                };
                stored.fiber_temporary_channel_id = outcome.temporary_channel_id;
                stored.channel_id = outcome.channel_id;
                stored.fiber_note = Some(
                    "LiquidLane submitted a vault-funded Fiber handoff. Vault liquidity is now waiting for Fiber confirmation."
                        .to_string(),
                );
                stored.fiber_error = None;
                let reference = stored
                    .channel_id
                    .clone()
                    .or_else(|| stored.fiber_temporary_channel_id.clone());
                let status = if stored.status == LiquidityStatus::ChannelOpen {
                    ExecutorJobStatus::ChannelActive
                } else {
                    ExecutorJobStatus::AwaitingFundingConfirmation
                };
                (
                    status,
                    None,
                    reference,
                    format!(
                        "Vault-funded Fiber handoff submitted for {}",
                        stored.merchant_name
                    ),
                )
            }
            Err(error) => {
                let message = error.to_string();
                stored.status = LiquidityStatus::Failed;
                stored.fiber_error = Some(message.clone());
                stored.fiber_note = Some(
                    "LiquidLane kept the vault reserve repairable; the Fiber handoff can be retried without asking LPs to sign again."
                        .to_string(),
                );
                (
                    ExecutorJobStatus::RetryableFailed,
                    Some(message),
                    stored.fiber_temporary_channel_id.clone(),
                    format!(
                        "Vault-funded Fiber handoff failed for {}",
                        stored.merchant_name
                    ),
                )
            }
        };
        stored.updated_at = now;
        let updated = stored.clone();
        update_intent_after_handoff(&mut state, &updated, now);
        update_reservation_and_positions(&mut state, &updated, now);
        state.events.insert(
            0,
            ActivityEvent {
                id: Uuid::new_v4(),
                actor_id: if executor {
                    updated.merchant_id
                } else {
                    actor_id
                },
                label,
                amount: Some(updated.amount),
                asset: Some(updated.asset.clone()),
                created_at: now,
            },
        );
        self.persist_locked(&state).await?;
        drop(state);

        if executor {
            let _ = self
                .mark_executor_job(request.id, job_status, job_error, job_ref)
                .await;
        }
        Ok(updated)
    }

    async fn mark_vault_funding_negotiating(
        &self,
        request: &LiquidityRequest,
        actor_id: Uuid,
        executor: bool,
    ) -> Result<()> {
        let updated = self
            .prepare_external_funding_intent(request, actor_id, executor)
            .await?;
        let mut state = self.inner.write().await;
        if let Some(stored) = state
            .liquidity_requests
            .iter_mut()
            .find(|stored| stored.id == updated.id)
        {
            stored.fiber_error = None;
            stored.fiber_note = Some(
                "LiquidLane is asking Fiber to build the channel using reserved LP vault liquidity."
                    .to_string(),
            );
            stored.updated_at = Utc::now();
        }
        self.persist_locked(&state).await
    }
}

fn update_intent_after_handoff(
    state: &mut super::StoreState,
    request: &LiquidityRequest,
    now: chrono::DateTime<Utc>,
) {
    if let Some(intent) = state
        .external_funding_intents
        .iter_mut()
        .find(|intent| intent.request_id == request.id)
    {
        intent.status = match request.status {
            LiquidityStatus::ChannelOpen => ExternalFundingIntentStatus::ChannelActive,
            LiquidityStatus::Failed => ExternalFundingIntentStatus::Failed,
            _ => ExternalFundingIntentStatus::FundingSubmitted,
        };
        intent.fiber_ref = request
            .channel_id
            .clone()
            .or_else(|| request.fiber_temporary_channel_id.clone());
        intent.note = request
            .fiber_note
            .clone()
            .unwrap_or_else(|| "Vault-funded Fiber handoff submitted.".to_string());
        intent.blockers.clear();
        intent.updated_at = now;
    }
}

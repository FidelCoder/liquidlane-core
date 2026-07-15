use anyhow::Result;
use chrono::Utc;
use uuid::Uuid;

use super::executor_channel_match::{
    matching_failed_channel, matching_settled_channel, matching_usable_channel,
};
use super::{
    AppStore,
    accounting::settle_positions,
    executor_channel_match::{
        funding_builder_timed_out, funding_negotiation_timed_out, funding_tx_timed_out,
        mark_intent_channel_active, matching_usable_channel_exact, record_final_funding,
        refresh_active_funding,
    },
    liquidity_deploy::update_reservation_and_positions,
};
use crate::domain::{
    ActivityEvent, ExecutorJobStatus, ExternalFundingIntentStatus, LiquidityStatus,
};

impl AppStore {
    pub async fn sync_fiber_channels(&self) -> Result<usize> {
        if !self.fiber.is_configured() {
            return Ok(0);
        }
        let mut channels = self.fiber.list_channels().await?;
        self.enrich_channel_funding_inputs(&mut channels).await;

        let now = Utc::now();
        let mut state = self.inner.write().await;
        let mut opened = Vec::new();
        let mut refreshed = Vec::new();
        let mut settled = Vec::new();
        let mut failed = Vec::new();
        let intents = state.external_funding_intents.clone();
        for request in state.liquidity_requests.iter_mut() {
            if request.status == LiquidityStatus::ChannelOpen {
                if let Some(channel) = matching_settled_channel(request, &intents, &channels) {
                    request.status = LiquidityStatus::Settled;
                    record_final_funding(request, channel);
                    request.channel_id = channel
                        .channel_id
                        .clone()
                        .or_else(|| request.channel_id.clone())
                        .or_else(|| request.fiber_temporary_channel_id.clone());
                    request.fiber_error = None;
                    request.fiber_note = Some(
                        "Fiber reports the channel settled. LP liquidity is available again."
                            .to_string(),
                    );
                    request.updated_at = now;
                    settled.push(request.clone());
                    continue;
                }
                if refresh_active_funding(request, &intents, &channels, now) {
                    refreshed.push(request.clone());
                }
            }

            let usable_channel = match request.status {
                LiquidityStatus::FundingSubmitted | LiquidityStatus::PendingFiberChannel => {
                    matching_usable_channel(request, &intents, &channels)
                }
                LiquidityStatus::Failed => {
                    matching_usable_channel_exact(request, &intents, &channels)
                }
                _ => None,
            };
            if let Some(channel) = usable_channel {
                request.status = LiquidityStatus::ChannelOpen;
                record_final_funding(request, channel);
                request.channel_id = channel
                    .channel_id
                    .clone()
                    .or_else(|| request.channel_id.clone())
                    .or_else(|| request.fiber_temporary_channel_id.clone());
                request.fiber_error = None;
                request.fiber_note = Some(
                    "Fiber channel confirmed. Merchant receive capacity is active.".to_string(),
                );
                request.updated_at = now;
                opened.push(request.clone());
                continue;
            }

            if matches!(
                request.status,
                LiquidityStatus::FundingSubmitted
                    | LiquidityStatus::PendingFiberChannel
                    | LiquidityStatus::ChannelOpen
            ) && matching_failed_channel(request, &intents, &channels).is_some()
            {
                request.status = LiquidityStatus::Failed;
                request.fiber_error = Some(
                    "Fiber funding attempt was aborted before the channel became active."
                        .to_string(),
                );
                request.fiber_note = Some("Liquidity remains reserved. Retry after the vault-funded Fiber transaction issue is fixed.".to_string());
                request.updated_at = now;
                failed.push(request.clone());
                continue;
            }

            if request.status == LiquidityStatus::FundingRequired
                && funding_builder_timed_out(request, &intents, now)
            {
                request.status = LiquidityStatus::Failed;
                request.fiber_error = Some(
                    "Vault funding builder timed out before a Fiber funding transaction was produced. Core did not receive a funding_tx_hash or funding_out_point for this reserve."
                        .to_string(),
                );
                request.fiber_note = Some(
                    "Vault liquidity remains reserved and repairable. Retry the Fiber handoff after the Fiber external-funding builder path is healthy."
                        .to_string(),
                );
                request.updated_at = now;
                tracing::warn!(
                    request_id = %request.id,
                    amount = request.amount,
                    asset = %request.asset,
                    status = ?request.status,
                    "vault-funded Fiber request timed out at builder_required stage"
                );
                failed.push(request.clone());
                continue;
            }

            if matches!(
                request.status,
                LiquidityStatus::FundingSubmitted | LiquidityStatus::PendingFiberChannel
            ) && funding_negotiation_timed_out(request, &intents, &channels, now)
            {
                request.status = LiquidityStatus::Failed;
                request.fiber_error = Some(
                    "Fiber funding negotiation timed out before a funding transaction outpoint was produced."
                        .to_string(),
                );
                request.fiber_note = Some(
                    "Vault liquidity remains reserved. Retry the Fiber handoff after the executor funding path is healthy."
                        .to_string(),
                );
                request.updated_at = now;
                tracing::warn!(
                    request_id = %request.id,
                    amount = request.amount,
                    asset = %request.asset,
                    "vault-funded Fiber request timed out while waiting for funding outpoint"
                );
                failed.push(request.clone());
                continue;
            }

            if matches!(
                request.status,
                LiquidityStatus::FundingSubmitted | LiquidityStatus::PendingFiberChannel
            ) && funding_tx_timed_out(request, &intents, now)
            {
                request.status = LiquidityStatus::Failed;
                request.fiber_error = Some(
                    "Fiber external funding timed out before a funding transaction hash was produced."
                        .to_string(),
                );
                request.fiber_note = Some(
                    "Vault liquidity remains reserved. Retry the Fiber handoff after the executor funding path is healthy."
                        .to_string(),
                );
                request.updated_at = now;
                failed.push(request.clone());
            }
        }

        for request in &opened {
            update_reservation_and_positions(&mut state, request, now);
            if let Some(job) = state
                .executor_jobs
                .iter_mut()
                .find(|job| job.request_id == request.id)
            {
                job.status = ExecutorJobStatus::ChannelActive;
                job.last_error = None;
                job.fiber_ref = request
                    .channel_id
                    .clone()
                    .or_else(|| request.fiber_temporary_channel_id.clone());
                job.updated_at = now;
            }
            mark_intent_channel_active(&mut state, request, now);
            state.events.insert(
                0,
                ActivityEvent {
                    id: Uuid::new_v4(),
                    actor_id: request.merchant_id,
                    label: format!("Fiber channel confirmed for {}", request.merchant_name),
                    amount: Some(request.amount),
                    asset: Some(request.asset.clone()),
                    created_at: now,
                },
            );
        }

        for request in &refreshed {
            mark_intent_channel_active(&mut state, request, now);
        }

        for request in &settled {
            if let Err(error) =
                settle_positions(&mut state.lp_positions, &request.asset, request.amount, now)
            {
                tracing::warn!(request_id = %request.id, error = %error, "failed to settle deployed LP liquidity from watcher");
            }
            if let Some(reservation) = state
                .capacity_reservations
                .iter_mut()
                .find(|reservation| reservation.request_id == request.id)
            {
                reservation.status = crate::domain::ReservationStatus::Released;
                reservation.updated_at = now;
            }
            if let Some(job) = state
                .executor_jobs
                .iter_mut()
                .find(|job| job.request_id == request.id)
            {
                job.status = ExecutorJobStatus::ChannelSettled;
                job.last_error = None;
                job.fiber_ref = request.channel_id.clone();
                job.updated_at = now;
            }
            state.events.insert(
                0,
                ActivityEvent {
                    id: Uuid::new_v4(),
                    actor_id: request.merchant_id,
                    label: format!("Fiber channel settled for {}", request.merchant_name),
                    amount: Some(request.amount),
                    asset: Some(request.asset.clone()),
                    created_at: now,
                },
            );
        }

        for request in &failed {
            update_reservation_and_positions(&mut state, request, now);
            if let Some(job) = state
                .executor_jobs
                .iter_mut()
                .find(|job| job.request_id == request.id)
            {
                job.status = ExecutorJobStatus::RetryableFailed;
                job.last_error = request.fiber_error.clone();
                job.updated_at = now;
            }
            for intent in state
                .external_funding_intents
                .iter_mut()
                .filter(|intent| intent.request_id == request.id)
            {
                intent.status = ExternalFundingIntentStatus::Failed;
                intent.blockers = request
                    .fiber_error
                    .clone()
                    .map(|error| vec![error])
                    .unwrap_or_default();
                intent.updated_at = now;
            }
            let failure_label = request
                .fiber_error
                .as_deref()
                .map(|error| {
                    format!(
                        "Fiber channel funding failed for {}: {error}",
                        request.merchant_name
                    )
                })
                .unwrap_or_else(|| {
                    format!("Fiber channel funding failed for {}", request.merchant_name)
                });
            state.events.insert(
                0,
                ActivityEvent {
                    id: Uuid::new_v4(),
                    actor_id: request.merchant_id,
                    label: failure_label,
                    amount: Some(request.amount),
                    asset: Some(request.asset.clone()),
                    created_at: now,
                },
            );
        }

        let changed = opened.len() + refreshed.len() + settled.len() + failed.len();
        if changed > 0 {
            self.persist_locked(&state).await?;
        }
        Ok(changed)
    }
}

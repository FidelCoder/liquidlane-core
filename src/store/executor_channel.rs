use anyhow::Result;
use chrono::Utc;
use uuid::Uuid;

use super::{
    AppStore, accounting::settle_positions, liquidity_deploy::update_reservation_and_positions,
};
use crate::{
    domain::{
        ActivityEvent, ExecutorJobStatus, ExternalFundingIntent, LiquidityRequest, LiquidityStatus,
    },
    fiber::FiberChannel,
};

impl AppStore {
    pub async fn sync_fiber_channels(&self) -> Result<usize> {
        if !self.fiber.is_configured() {
            return Ok(0);
        }
        let channels = self.fiber.list_channels().await?;
        if channels.is_empty() {
            return Ok(0);
        }

        let now = Utc::now();
        let mut state = self.inner.write().await;
        let mut opened = Vec::new();
        let mut settled = Vec::new();
        let mut failed = Vec::new();
        let intents = state.external_funding_intents.clone();
        for request in state.liquidity_requests.iter_mut() {
            if request.status == LiquidityStatus::ChannelOpen {
                if let Some(channel) = matching_settled_channel(request, &intents, &channels) {
                    request.status = LiquidityStatus::Settled;
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
            }

            if matches!(
                request.status,
                LiquidityStatus::FundingSubmitted | LiquidityStatus::PendingFiberChannel
            ) {
                if let Some(channel) = matching_usable_channel(request, &intents, &channels) {
                    request.status = LiquidityStatus::ChannelOpen;
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
            state.events.insert(
                0,
                ActivityEvent {
                    id: Uuid::new_v4(),
                    actor_id: request.merchant_id,
                    label: format!("Fiber channel funding failed for {}", request.merchant_name),
                    amount: Some(request.amount),
                    asset: Some(request.asset.clone()),
                    created_at: now,
                },
            );
        }

        let changed = opened.len() + settled.len() + failed.len();
        if changed > 0 {
            self.persist_locked(&state).await?;
        }
        Ok(changed)
    }
}

pub(super) fn matching_usable_channel<'a>(
    request: &LiquidityRequest,
    intents: &[ExternalFundingIntent],
    channels: &'a [FiberChannel],
) -> Option<&'a FiberChannel> {
    channels
        .iter()
        .find(|channel| channel.is_usable && channel_matches_request(request, intents, channel))
}

pub(super) fn matching_settled_channel<'a>(
    request: &LiquidityRequest,
    intents: &[ExternalFundingIntent],
    channels: &'a [FiberChannel],
) -> Option<&'a FiberChannel> {
    channels.iter().find(|channel| {
        channel.is_closed
            && !channel.is_failed
            && channel_matches_request(request, intents, channel)
    })
}

pub(super) fn matching_failed_channel<'a>(
    request: &LiquidityRequest,
    intents: &[ExternalFundingIntent],
    channels: &'a [FiberChannel],
) -> Option<&'a FiberChannel> {
    channels
        .iter()
        .find(|channel| channel.is_failed && channel_matches_request(request, intents, channel))
}

pub(super) fn channel_matches_request(
    request: &LiquidityRequest,
    intents: &[ExternalFundingIntent],
    channel: &FiberChannel,
) -> bool {
    let intent = intents
        .iter()
        .find(|intent| intent.request_id == request.id);
    intent.is_some_and(|intent| {
        matches_ref(
            channel.funding_tx_hash.as_deref(),
            intent.funding_tx_hash.as_deref(),
        ) || matches_ref(
            channel.funding_out_point.as_deref(),
            intent.funding_out_point.as_deref(),
        )
    }) || matches_ref(channel.channel_id.as_deref(), request.channel_id.as_deref())
        || matches_ref(
            channel.temporary_channel_id.as_deref(),
            request.fiber_temporary_channel_id.as_deref(),
        )
        || request
            .fiber_peer_pubkey
            .as_deref()
            .zip(channel.peer_pubkey.as_deref())
            .is_some_and(|(request_peer, channel_peer)| {
                request_peer.eq_ignore_ascii_case(channel_peer)
                    && channel.amount_ckb == Some(request.amount)
            })
}

fn matches_ref(left: Option<&str>, right: Option<&str>) -> bool {
    left.zip(right)
        .is_some_and(|(left, right)| !left.is_empty() && left.eq_ignore_ascii_case(right))
}

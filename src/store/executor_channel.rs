use anyhow::Result;
use chrono::Utc;
use uuid::Uuid;

use super::{AppStore, liquidity_deploy::update_reservation_and_positions};
use crate::{
    domain::{ActivityEvent, ExecutorJobStatus, LiquidityRequest, LiquidityStatus},
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
        let mut failed = Vec::new();
        for request in state.liquidity_requests.iter_mut() {
            if matches!(
                request.status,
                LiquidityStatus::FundingSubmitted | LiquidityStatus::PendingFiberChannel
            ) {
                if let Some(channel) = matching_usable_channel(request, &channels) {
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
            ) && matching_failed_channel(request, &channels).is_some()
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

        let changed = opened.len() + failed.len();
        if changed > 0 {
            self.persist_locked(&state).await?;
        }
        Ok(changed)
    }
}

fn matching_usable_channel<'a>(
    request: &LiquidityRequest,
    channels: &'a [FiberChannel],
) -> Option<&'a FiberChannel> {
    channels
        .iter()
        .find(|channel| channel.is_usable && channel_matches_request(request, channel))
}

fn matching_failed_channel<'a>(
    request: &LiquidityRequest,
    channels: &'a [FiberChannel],
) -> Option<&'a FiberChannel> {
    channels
        .iter()
        .find(|channel| channel.is_failed && channel_matches_request(request, channel))
}

fn channel_matches_request(request: &LiquidityRequest, channel: &FiberChannel) -> bool {
    matches_ref(channel.channel_id.as_deref(), request.channel_id.as_deref())
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

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use uuid::Uuid;

    use super::{channel_matches_request, matching_failed_channel, matching_usable_channel};
    use crate::{domain::LiquidityStatus, fiber::FiberChannel};

    #[test]
    fn usable_channel_matches_temporary_ref() {
        let mut request = request(200);
        request.fiber_temporary_channel_id = Some("0xtemp".to_string());
        let channels = vec![channel(Some("0xtemp"), None, Some(500), true, false)];

        assert!(matching_usable_channel(&request, &channels).is_some());
    }

    #[test]
    fn peer_match_requires_exact_reserved_amount() {
        let request = request(200);
        let wrong_amount = channel(None, Some("03peer"), Some(100), true, false);
        let right_amount = channel(None, Some("03peer"), Some(200), true, false);

        assert!(!channel_matches_request(&request, &wrong_amount));
        assert!(channel_matches_request(&request, &right_amount));
    }

    #[test]
    fn failed_channel_matches_pending_request_by_amount() {
        let request = request(500);
        let channels = vec![channel(None, Some("03peer"), Some(500), false, true)];

        assert!(matching_failed_channel(&request, &channels).is_some());
    }

    fn request(amount: u64) -> crate::domain::LiquidityRequest {
        let now = Utc::now();
        crate::domain::LiquidityRequest {
            id: Uuid::new_v4(),
            merchant_id: Uuid::new_v4(),
            merchant_name: "Recovered merchant".to_string(),
            ckb_address: "ckt1qmerchant".to_string(),
            asset: "CKB".to_string(),
            amount,
            duration_days: 1,
            lease_fee: 1,
            routing_fee_bps: 30,
            fiber_peer_pubkey: Some("03peer".to_string()),
            fiber_peer_address: None,
            public_channel: false,
            funding_udt_type_script: None,
            request_cell_id: "ll-request-test".to_string(),
            request_tx_hash: None,
            request_cell_out_point: None,
            status: LiquidityStatus::PendingFiberChannel,
            fiber_temporary_channel_id: None,
            channel_id: None,
            fiber_note: None,
            fiber_error: None,
            created_at: now,
            updated_at: now,
        }
    }

    fn channel(
        temporary_channel_id: Option<&str>,
        peer_pubkey: Option<&str>,
        amount_ckb: Option<u64>,
        is_usable: bool,
        is_failed: bool,
    ) -> FiberChannel {
        FiberChannel {
            channel_id: None,
            temporary_channel_id: temporary_channel_id.map(str::to_string),
            peer_pubkey: peer_pubkey.map(str::to_string),
            amount_ckb,
            is_usable,
            is_failed,
        }
    }
}

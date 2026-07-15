use chrono::{DateTime, Duration, Utc};
use serde_json::Value;

use super::AppStore;
use crate::{
    domain::{ExternalFundingIntent, ExternalFundingIntentStatus, LiquidityRequest},
    fiber::FiberChannel,
};

const FUNDING_TX_TIMEOUT: Duration = Duration::minutes(2);

impl AppStore {
    pub(super) async fn enrich_channel_funding_inputs(&self, channels: &mut [FiberChannel]) {
        let Some(rpc) = self.ckb_rpc.as_ref() else {
            return;
        };
        for channel in channels {
            let Some(tx_hash) = channel.funding_tx_hash.as_deref() else {
                continue;
            };
            match rpc.transaction_details(tx_hash).await {
                Ok(details) => {
                    channel.funding_input_out_points =
                        transaction_input_out_points(&details.transaction);
                }
                Err(error) => {
                    tracing::debug!(tx_hash, error = %error, "could not inspect Fiber funding transaction");
                }
            }
        }
    }
}

pub(super) fn record_final_funding(request: &mut LiquidityRequest, channel: &FiberChannel) {
    if let Some(tx_hash) = channel.funding_tx_hash.as_ref() {
        request.funding_tx_hash = Some(tx_hash.clone());
    }
    if let Some(out_point) = channel.funding_out_point.as_ref() {
        request.funding_out_point = Some(out_point.clone());
    }
    if let Some(usable_capacity) = channel.amount_ckb {
        request.usable_capacity = usable_capacity;
    }
}

pub(super) fn refresh_active_funding(
    request: &mut LiquidityRequest,
    intents: &[ExternalFundingIntent],
    channels: &[FiberChannel],
    now: DateTime<Utc>,
) -> bool {
    let Some(channel) = matching_usable_channel_exact(request, intents, channels) else {
        return false;
    };
    let previous = (
        request.funding_tx_hash.clone(),
        request.funding_out_point.clone(),
        request.usable_capacity,
    );
    record_final_funding(request, channel);
    let changed = previous
        != (
            request.funding_tx_hash.clone(),
            request.funding_out_point.clone(),
            request.usable_capacity,
        );
    if changed {
        request.updated_at = now;
    }
    changed
}

pub(super) fn mark_intent_channel_active(
    state: &mut super::StoreState,
    request: &LiquidityRequest,
    now: DateTime<Utc>,
) {
    if let Some(intent) = state
        .external_funding_intents
        .iter_mut()
        .find(|intent| intent.request_id == request.id)
    {
        intent.status = ExternalFundingIntentStatus::ChannelActive;
        intent.funding_tx_hash = request.funding_tx_hash.clone();
        intent.funding_out_point = request.funding_out_point.clone();
        intent.fiber_ref = request.channel_id.clone();
        intent.note =
            "Fiber channel confirmed with the final collaborative funding transaction.".to_string();
        intent.blockers.clear();
        intent.updated_at = now;
    }
}

fn transaction_input_out_points(transaction: &Value) -> Vec<String> {
    transaction
        .get("inputs")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|input| {
            let previous = input.get("previous_output")?;
            Some(format!(
                "{}#{}",
                previous.get("tx_hash")?.as_str()?,
                previous.get("index")?.as_str()?
            ))
        })
        .collect()
}

pub(super) fn funding_builder_timed_out(
    request: &LiquidityRequest,
    intents: &[ExternalFundingIntent],
    now: DateTime<Utc>,
) -> bool {
    let Some(intent) = intents
        .iter()
        .find(|intent| intent.request_id == request.id)
    else {
        return now.signed_duration_since(request.updated_at) >= FUNDING_TX_TIMEOUT;
    };
    if intent.status != ExternalFundingIntentStatus::BuilderRequired {
        return false;
    }
    if intent.funding_tx_hash.is_some() || intent.funding_out_point.is_some() {
        return false;
    }
    now.signed_duration_since(intent.updated_at) >= FUNDING_TX_TIMEOUT
}

pub(super) fn funding_tx_timed_out(
    request: &LiquidityRequest,
    intents: &[ExternalFundingIntent],
    now: DateTime<Utc>,
) -> bool {
    let Some(intent) = intents
        .iter()
        .find(|intent| intent.request_id == request.id)
    else {
        return false;
    };
    if intent.status != ExternalFundingIntentStatus::FundingSubmitted {
        return false;
    }
    if intent.funding_tx_hash.is_some() {
        return false;
    }
    now.signed_duration_since(intent.updated_at) >= FUNDING_TX_TIMEOUT
}

pub(super) fn funding_negotiation_timed_out(
    request: &LiquidityRequest,
    intents: &[ExternalFundingIntent],
    channels: &[FiberChannel],
    now: DateTime<Utc>,
) -> bool {
    if now.signed_duration_since(request.updated_at) < FUNDING_TX_TIMEOUT {
        return false;
    }
    matching_unfunded_channel(request, intents, channels).is_some()
}

fn matching_unfunded_channel<'a>(
    request: &LiquidityRequest,
    intents: &[ExternalFundingIntent],
    channels: &'a [FiberChannel],
) -> Option<&'a FiberChannel> {
    channels.iter().find(|channel| {
        !channel.is_usable
            && !channel.is_failed
            && !channel.is_closed
            && channel.funding_tx_hash.is_none()
            && channel.funding_out_point.is_none()
            && channel_matches_request(request, intents, channel)
    })
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
    channels.iter().find(|channel| {
        channel.is_failed && channel_identity_matches_request(request, intents, channel)
    })
}

pub(super) fn matching_usable_channel_exact<'a>(
    request: &LiquidityRequest,
    intents: &[ExternalFundingIntent],
    channels: &'a [FiberChannel],
) -> Option<&'a FiberChannel> {
    channels.iter().find(|channel| {
        channel.is_usable && channel_identity_matches_request(request, intents, channel)
    })
}

fn channel_identity_matches_request(
    request: &LiquidityRequest,
    intents: &[ExternalFundingIntent],
    channel: &FiberChannel,
) -> bool {
    let spends_request_cell = request
        .request_cell_out_point
        .as_deref()
        .is_some_and(|out_point| {
            channel
                .funding_input_out_points
                .iter()
                .any(|input| input.eq_ignore_ascii_case(out_point))
        });
    let intent = intents
        .iter()
        .find(|intent| intent.request_id == request.id);
    spends_request_cell
        || intent.is_some_and(|intent| {
            matches_ref(
                channel.funding_tx_hash.as_deref(),
                intent.funding_tx_hash.as_deref(),
            ) || matches_ref(
                channel.funding_out_point.as_deref(),
                intent.funding_out_point.as_deref(),
            )
        })
        || matches_ref(channel.channel_id.as_deref(), request.channel_id.as_deref())
        || matches_ref(
            channel.channel_id.as_deref(),
            request.fiber_temporary_channel_id.as_deref(),
        )
        || matches_ref(
            channel.temporary_channel_id.as_deref(),
            request.fiber_temporary_channel_id.as_deref(),
        )
}

pub(super) fn channel_matches_request(
    request: &LiquidityRequest,
    intents: &[ExternalFundingIntent],
    channel: &FiberChannel,
) -> bool {
    channel_identity_matches_request(request, intents, channel)
}

fn matches_ref(left: Option<&str>, right: Option<&str>) -> bool {
    left.zip(right)
        .is_some_and(|(left, right)| !left.is_empty() && left.eq_ignore_ascii_case(right))
}

use super::{Connector, Journal, execution::channel_state};
use crate::marketplace::{
    chain,
    model::{ProviderPolicy, SHANNONS},
};
use anyhow::{Context, Result, ensure};
use serde_json::Value;
use std::collections::HashSet;

impl Connector {
    pub async fn provider_policy(&self, channels: &[Value]) -> Result<ProviderPolicy> {
        let journals = self.db.list::<Journal>("journal")?;
        let settled = self.settled_channels(channels, &journals).await;
        Ok(ProviderPolicy {
            accept_public_orders: self.config.accept_public_orders,
            auto_approve: self.config.auto_approve,
            max_order_ckb: self.config.max_order_ckb,
            max_total_ckb: self.config.max_total_ckb,
            min_fee_ckb: self.config.min_fee_ckb,
            committed_ckb: exposure(channels, &journals, &settled)?,
        })
    }
}

pub(super) fn matches(journal: &Journal, channel: &Value) -> bool {
    journal
        .channel_id
        .as_deref()
        .is_some_and(|id| channel["channel_id"] == id)
        || journal
            .funding_outpoint
            .as_deref()
            .and_then(|id| chain::canonical_outpoint(id).ok())
            .is_some_and(|id| {
                channel["channel_outpoint"]
                    .as_str()
                    .and_then(|s| chain::canonical_outpoint(s).ok())
                    .as_ref()
                    == Some(&id)
            })
}

fn recovered(point: Option<&str>, settled: &HashSet<String>) -> bool {
    point
        .and_then(|s| chain::canonical_outpoint(s).ok())
        .is_some_and(|s| settled.contains(&s))
}

pub fn exposure(
    channels: &[Value],
    journals: &[Journal],
    settled: &HashSet<String>,
) -> Result<u64> {
    let mut total = 0u64;
    for channel in channels
        .iter()
        .filter(|c| !recovered(c["channel_outpoint"].as_str(), settled))
    {
        // Retain the opening commitment even after payments move its balance. Count all
        // native channels, including channels opened outside LiquidLane, conservatively.
        let current = chain::number(&channel["local_balance"])?
            .div_ceil(SHANNONS)
            .checked_add(99)
            .context("channel exposure overflow")?;
        let committed = journals
            .iter()
            .filter(|j| j.attempted && matches(j, channel))
            .map(|j| j.funding_ckb)
            .max()
            .unwrap_or(0);
        total = total
            .checked_add(current.max(committed))
            .context("channel exposure overflow")?;
    }
    for journal in journals.iter().filter(|j| {
        j.attempted
            && !recovered(j.funding_outpoint.as_deref(), settled)
            && !channels.iter().any(|c| matches(j, c))
    }) {
        ensure!(
            journal.funding_ckb > 0,
            "an earlier opening needs reconciliation before new funding"
        );
        total = total
            .checked_add(journal.funding_ckb)
            .context("channel exposure overflow")?;
    }
    Ok(total)
}

pub fn opening_in_progress(
    channels: &[Value],
    journals: &[Journal],
    settled: &HashSet<String>,
) -> bool {
    journals.iter().any(|j| {
        j.attempted
            && !recovered(j.funding_outpoint.as_deref(), settled)
            && !channels.iter().any(|c| {
                matches(j, c)
                    && matches!(channel_state(c), "ChannelReady" | "ShuttingDown" | "Closed")
            })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn channel(id: &str, state: &str, balance: u64) -> Value {
        json!({"channel_id":id,"state":state,"local_balance":format!("0x{:x}",balance * SHANNONS)})
    }
    fn journal() -> Journal {
        Journal {
            attempted: true,
            channel_id: Some("channel".into()),
            funding_ckb: 600,
            ..Default::default()
        }
    }
    #[test]
    fn payments_and_unverified_closure_do_not_free_committed_budget() {
        let journals = [journal()];
        assert_eq!(
            exposure(
                &[channel("channel", "ChannelReady", 1)],
                &journals,
                &HashSet::new()
            )
            .unwrap(),
            600
        );
        assert_eq!(
            exposure(
                &[channel("channel", "Closed", 1)],
                &journals,
                &HashSet::new()
            )
            .unwrap(),
            600
        );
        assert_eq!(
            exposure(
                &[channel("other", "ChannelReady", 101)],
                &journals,
                &HashSet::new()
            )
            .unwrap(),
            800
        );
    }
    #[test]
    fn only_chain_settlement_releases_stale_or_missing_channels() {
        let hash = format!("0x{}", "12".repeat(32));
        let canonical = format!("{hash}#0");
        let packed = format!("{hash}00000000");
        let mut journal = journal();
        journal.funding_outpoint = Some(canonical.clone());
        journal.channel_id = Some("temporary-id".into());
        let journals = [journal];
        let mut native = channel("channel", "ShuttingDown", 101);
        native["channel_outpoint"] = json!(packed);
        assert!(matches(&journals[0], &native));
        assert_eq!(
            exposure(&[native.clone()], &journals, &HashSet::new()).unwrap(),
            600
        );
        let settled = HashSet::from([canonical]);
        assert_eq!(exposure(&[native], &journals, &settled).unwrap(), 0);
        assert_eq!(exposure(&[], &journals, &settled).unwrap(), 0);
        assert!(!opening_in_progress(&[], &journals, &settled));
    }
    #[test]
    fn an_uncertain_opening_keeps_its_budget_and_blocks_another_open() {
        let journals = [journal()];
        assert_eq!(exposure(&[], &journals, &HashSet::new()).unwrap(), 600);
        assert!(opening_in_progress(&[], &journals, &HashSet::new()));
        assert!(opening_in_progress(
            &[channel("channel", "AwaitingTxSignatures", 501)],
            &journals,
            &HashSet::new()
        ));
        assert!(!opening_in_progress(
            &[channel("channel", "ChannelReady", 501)],
            &journals,
            &HashSet::new()
        ));
        let legacy = Journal {
            funding_ckb: 0,
            ..journal()
        };
        assert!(exposure(&[], &[legacy], &HashSet::new()).is_err());
    }
}

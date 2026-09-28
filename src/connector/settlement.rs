use super::{Connector, Journal, execution::channel_state};
use crate::marketplace::{
    chain, db,
    model::now,
    settlement::{self, Settlement},
};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;

#[derive(Serialize, Deserialize)]
struct Check {
    checked_at: i64,
    settlement: Option<Settlement>,
}

impl Connector {
    pub async fn settlement(&self, outpoint: &str) -> Result<Option<Settlement>> {
        let outpoint = chain::canonical_outpoint(outpoint)?;
        if let Ok(check) = self.db.get::<Check>("settlement", &outpoint) {
            if now().abs_diff(check.checked_at) < 60 {
                return Ok(check.settlement);
            }
        }
        let result = settlement::inspect(&self.client, &self.config.ckb_rpc, &outpoint).await;
        // An RPC outage must never renew an old confirmation. Cache failure as unknown
        // so a failing indexer is not hammered by every order and policy calculation.
        let check = Check {
            checked_at: now(),
            settlement: result.as_ref().ok().cloned().flatten(),
        };
        self.db
            .transaction(|tx| db::put(tx, "settlement", &outpoint, "local", &check))?;
        result
    }

    pub async fn settled_channels(
        &self,
        channels: &[Value],
        journals: &[Journal],
    ) -> HashSet<String> {
        let mut candidates: HashSet<String> = channels
            .iter()
            .filter(|c| matches!(channel_state(c), "Closed" | "ShuttingDown"))
            .filter_map(|c| c["channel_outpoint"].as_str())
            .filter_map(|s| chain::canonical_outpoint(s).ok())
            .collect();
        for journal in journals.iter().filter(|j| j.attempted) {
            if let Some(point) = &journal.funding_outpoint {
                if !channels.iter().any(|c| super::budget::matches(journal, c)) {
                    if let Ok(point) = chain::canonical_outpoint(point) {
                        candidates.insert(point);
                    }
                }
            }
        }
        let mut settled = HashSet::new();
        for point in candidates {
            match self.settlement(&point).await {
                Ok(Some(proof)) if proof.confirmed => {
                    settled.insert(point);
                }
                Err(error) => tracing::warn!(%point,%error,"native settlement remains unverified"),
                _ => {}
            }
        }
        settled
    }
}

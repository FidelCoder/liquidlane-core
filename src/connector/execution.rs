use super::{Connector, Journal, budget};
use crate::marketplace::{
    chain,
    model::{Order, SHANNONS, now},
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};

impl Connector {
    pub async fn open_order(&self, order: &Order) -> Result<()> {
        let mut journal = self.journal(&order.id)?;
        if journal.attempted {
            return Ok(());
        }
        self.validate_order(order, true).await?;
        let channels = self.channels().await?;
        let journals = self.db.list::<Journal>("journal")?;
        let settled = self.settled_channels(&channels, &journals).await;
        ensure!(
            !budget::opening_in_progress(&channels, &journals, &settled),
            "waiting for the previous channel opening to resolve"
        );
        let exposure = budget::exposure(&channels, &journals, &settled)?;
        ensure!(
            order.quote.funding_ckb <= self.config.max_total_ckb.saturating_sub(exposure),
            "total channel exposure exceeds local policy"
        );
        let info = self.node_info().await?;
        ensure!(
            self.available_ckb(&info).await? >= order.quote.funding_ckb + 63,
            "insufficient local funding before opening"
        );
        self.fiber("connect_peer",json!([{"address":order.quote.merchant_address,"pubkey":order.quote.merchant_pubkey,"save":true}])).await?;
        let peers = self.fiber("list_peers", json!([])).await?;
        ensure!(
            peers["peers"]
                .as_array()
                .context("peer list missing")?
                .iter()
                .any(|p| p["pubkey"] == order.quote.merchant_pubkey),
            "waiting for merchant peer handshake"
        );
        let info = self.node_info().await?;
        let shutdown = info
            .get("default_funding_lock_script")
            .context("local closing lock unavailable")?
            .clone();
        let accepted = self
            .post_signed(
                &format!("/connector/orders/{}/start", order.id),
                json!({"order_id":order.id,"action":"start"}),
            )
            .await?;
        ensure!(
            accepted["quote_hash"] == order.quote_hash,
            "coordinator changed accepted quote"
        );
        journal.quote_hash = order.quote_hash.clone();
        journal.attempted = true;
        journal.funding_ckb = order.quote.funding_ckb;
        journal.started_at = now();
        journal.baseline_channels = channels
            .iter()
            .filter_map(|c| c["channel_id"].as_str().map(str::to_owned))
            .collect();
        self.save(&journal)?;
        let params = json!([{"pubkey":order.quote.merchant_pubkey,"funding_amount":format!("0x{:x}",order.quote.funding_ckb*SHANNONS),"public":order.quote.public_channel,"one_way":false,"shutdown_script":shutdown,"funding_fee_rate":"0x7d0"}]);
        match self.fiber("open_channel", params).await {
            Ok(result) => {
                journal.channel_id = result["temporary_channel_id"]
                    .as_str()
                    .or(result["channel_id"].as_str())
                    .map(str::to_owned);
                journal.error = None;
                self.save(&journal)?;
                tracing::info!(order_id=%order.id,channel_id=?journal.channel_id,"native provider-funded channel requested");
            }
            Err(error) => {
                journal.error = Some(error.to_string());
                self.save(&journal)?;
                let _=self.post_signed(&format!("/connector/orders/{}/failure",order.id),json!({"order_id":order.id,"error":error.to_string().chars().take(500).collect::<String>(),"before_funding":false})).await;
                return Err(error);
            }
        }
        Ok(())
    }
    pub async fn locate_channel(&self, order: &Order, journal: &Journal) -> Result<Option<Value>> {
        let channels = self.channels().await?;
        let peer = if self.registration.node.role == "provider" {
            &order.quote.merchant_pubkey
        } else {
            &order.quote.provider_pubkey
        };
        let mut matches: Vec<Value> = channels
            .into_iter()
            .filter(|c| {
                c["pubkey"] == *peer
                    && c["is_one_way"] == false
                    && c["funding_udt_type_script"].is_null()
            })
            .collect();
        if let Some(id) = &order.channel_id {
            return Ok(matches.into_iter().find(|c| c["channel_id"] == *id));
        }
        if let Some(id) = &journal.channel_id {
            if let Some(channel) = matches.iter().find(|c| c["channel_id"] == *id) {
                return Ok(Some(channel.clone()));
            }
        }
        matches.retain(|c| {
            let Some(id) = c["channel_id"].as_str() else {
                return false;
            };
            if journal.baseline_channels.iter().any(|known| known == id) {
                return false;
            }
            let created = chain::number(&c["created_at"]).unwrap_or(0) / 1000;
            created >= order.created_at.saturating_sub(5) as u64
        });
        ensure!(
            matches.len() <= 1,
            "multiple candidate channels; inspect local state before matching this order"
        );
        Ok(matches.pop())
    }
}
pub fn channel_state(channel: &Value) -> &str {
    channel["state"]["state_name"]
        .as_str()
        .or(channel["state"].as_str())
        .unwrap_or("Unknown")
}

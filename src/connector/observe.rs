use super::{Connector, execution::channel_state};
use crate::marketplace::{
    chain, crypto,
    model::{ChannelEvidence, Order, SHANNONS, now},
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};

impl Connector {
    pub async fn observe(&self, order: &Order) -> Result<()> {
        let mut journal = self.journal(&order.id)?;
        let Some(channel) = self.locate_channel(order, &journal).await? else {
            return Ok(());
        };
        let Some(outpoint) = channel["channel_outpoint"].as_str() else {
            return Ok(());
        };
        let mut evidence = channel_evidence(&channel)?;
        if matches!(evidence.state.as_str(), "Closed" | "ShuttingDown") {
            match self.settlement(outpoint).await {
                Ok(settlement) => evidence.settlement = settlement,
                Err(error) => {
                    tracing::warn!(order_id=%order.id,%error,"settlement check unavailable")
                }
            }
        }
        evidence.observed_at = now();
        journal.channel_id = Some(evidence.channel_id.clone());
        journal.funding_outpoint = Some(outpoint.into());
        if self.registration.node.role == "provider"
            && journal.attempted
            && journal.funding_ckb == 0
        {
            ensure!(
                journal.quote_hash == order.quote_hash
                    && order.quote_hash
                        == crypto::digest(crypto::quote_message(&order.quote)?.as_bytes()),
                "observed quote differs from the local funding commitment"
            );
            journal.funding_ckb = order.quote.funding_ckb;
        }
        self.save(&journal)?;
        self.post_signed(
            &format!("/connector/orders/{}/evidence", order.id),
            json!({"order_id":order.id,"evidence":evidence}),
        )
        .await?;
        if self.registration.node.role != "merchant"
            || channel_state(&channel) != "ChannelReady"
            || order.probe_received
        {
            return Ok(());
        }
        if journal.invoice.is_none() {
            // Persist before advertising the invoice so a restart cannot substitute a new probe.
            let invoice = self.fiber("new_invoice", json!([{"amount":format!("0x{SHANNONS:x}"),"currency":"Fibt","description":format!("LiquidLane delivery {}", order.id),"expiry":"0x15180","allow_mpp":false}])).await?;
            journal.invoice = Some(invoice);
            self.save(&journal)?;
        }
        let invoice = journal.invoice.as_ref().context("probe invoice missing")?;
        let hash = invoice["invoice"]["data"]["payment_hash"]
            .as_str()
            .context("Fiber returned no invoice payment hash")?;
        let address = invoice["invoice_address"]
            .as_str()
            .context("Fiber returned no invoice address")?;
        let status = self
            .fiber("get_invoice", json!([{"payment_hash":hash}]))
            .await?;
        if order.probe_payment_hash.is_none() || status["status"] == "Paid" {
            self.post_signed(&format!("/connector/orders/{}/probe", order.id), json!({"order_id":order.id,"invoice":address,"payment_hash":hash,"paid":status["status"]=="Paid"})).await?;
        }
        Ok(())
    }
}

pub fn channel_evidence(channel: &Value) -> Result<ChannelEvidence> {
    ensure!(
        channel["is_one_way"] == false,
        "one-way channels cannot deliver marketplace capacity"
    );
    ensure!(
        channel["funding_udt_type_script"].is_null(),
        "only native CKB channels are supported"
    );
    let text = |key: &str| -> Result<String> {
        Ok(channel[key]
            .as_str()
            .with_context(|| format!("channel field {key} missing"))?
            .into())
    };
    let remote = chain::number(&channel["remote_balance"])?;
    let pending = chain::number(&channel["received_tlc_balance"])?;
    // v0.9.0 remote_balance already excludes the remote channel reserve.
    let inbound = if channel_state(channel) == "ChannelReady" {
        remote
            .checked_sub(pending)
            .context("pending incoming TLC balance exceeds remote balance")?
    } else {
        0
    };
    Ok(ChannelEvidence {
        channel_id: text("channel_id")?,
        peer_pubkey: text("pubkey")?,
        funding_outpoint: text("channel_outpoint")?,
        state: channel_state(channel).into(),
        local_balance: text("local_balance")?,
        remote_balance: text("remote_balance")?,
        inbound_liquidity: format!("0x{inbound:x}"),
        observed_at: now(),
        settlement_tx_hash: channel["shutdown_transaction_hash"]
            .as_str()
            .map(str::to_owned),
        state_flags: channel["state"]["state_flags"].as_str().map(str::to_owned),
        settlement: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn spendable_remote_balance_deducts_pending_tlcs_but_not_reserve_twice() {
        let mut channel = json!({"channel_id":"channel","pubkey":"peer","channel_outpoint":"outpoint","is_one_way":false,"funding_udt_type_script":null,"state":{"state_name":"ChannelReady"},"local_balance":"0x0","remote_balance":"0xba43b7400","received_tlc_balance":"0x5f5e100"});
        let evidence = channel_evidence(&channel).unwrap();
        assert_eq!(
            chain::number(&json!(evidence.inbound_liquidity)).unwrap(),
            499 * SHANNONS
        );
        channel["state"]["state_name"] = json!("Closed");
        assert_eq!(channel_evidence(&channel).unwrap().inbound_liquidity, "0x0");
        channel["is_one_way"] = json!(true);
        assert!(channel_evidence(&channel).is_err());
    }
}

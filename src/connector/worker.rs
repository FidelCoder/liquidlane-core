use super::Connector;
use crate::marketplace::{
    chain, crypto,
    model::{Order, TESTNET_GENESIS, now},
};
use anyhow::{Context, Result, ensure};
use serde_json::json;

impl Connector {
    pub async fn tick(&self) -> Result<()> {
        let lock = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(self.config.state_dir.join("worker.lock"))?;
        lock.try_lock()
            .context("another connector cycle is already running for this state directory")?;
        let info = self.node_info().await?;
        let available = self.available_ckb(&info).await?;
        let result = self.core("/connector/orders", None).await?;
        let orders: Vec<Order> = serde_json::from_value(result["orders"].clone())?;
        // Reconcile previous attempts before using their reservations for new funding.
        for order in orders.iter().filter(|o| {
            matches!(
                o.status.as_str(),
                "opening" | "awaiting_confirmation" | "verifying" | "delivered" | "reconciling"
            )
        }) {
            if let Err(error) = self.observe(order).await {
                tracing::warn!(order_id=%order.id,%error,"channel observation needs attention");
            }
        }
        let reserve = if self.registration.node.role == "merchant" {
            chain::number(&info["auto_accept_channel_ckb_funding_amount"])?
                / crate::marketplace::model::SHANNONS
        } else {
            99
        };
        ensure!(
            reserve >= 99,
            "receiving node auto-accept funding must cover its native channel reserve"
        );
        let policy = if self.registration.node.role == "provider" {
            Some(self.provider_policy(&self.channels().await?).await?)
        } else {
            None
        };
        let funding_address = crypto::funding_address(&info["default_funding_lock_script"])?;
        self.post_signed("/connector/heartbeat",json!({"available_ckb":available,"reserve_ckb":reserve,"version":self.config.fiber_version,"chain_hash":TESTNET_GENESIS,"provider_policy":policy,"funding_address":funding_address,"background":std::env::var_os("INVOCATION_ID").is_some()})).await?;
        for order in &orders {
            if let Err(error) = self.process(order).await {
                tracing::warn!(order_id=%order.id,%error,"order needs attention");
            }
        }
        Ok(())
    }
    async fn process(&self, order: &Order) -> Result<()> {
        if self.registration.node.role == "provider" {
            if order.status == "awaiting_quote" {
                self.validate_order(order, false).await?;
                let signature = crypto::sign(&self.secret, &crypto::quote_message(&order.quote)?);
                self.core(
                    &format!("/connector/orders/{}/quote", order.id),
                    Some(json!({"signature":signature})),
                )
                .await?;
                tracing::info!(order_id=%order.id,"quoted under provider policy");
                return Ok(());
            }
            if order.status == "accepted" {
                self.validate_order(order, true).await?;
                let journal = self.journal(&order.id)?;
                if !self.config.auto_approve && !journal.approved {
                    return Ok(());
                }
                ensure!(
                    journal.quote_hash.is_empty() || journal.quote_hash == order.quote_hash,
                    "local approval belongs to another quote"
                );
                self.open_order(order).await?;
            }
        }
        Ok(())
    }
    pub async fn inspect(&self) -> Result<()> {
        let info = self.node_info().await?;
        let available = self.available_ckb(&info).await?;
        let orders = self.core("/connector/orders", None).await?;
        println!(
            "{}",
            serde_json::to_string_pretty(
                &json!({"node_id":self.registration.node.id,"pubkey":info["pubkey"],"role":self.registration.node.role,"version":info["version"],"available_ckb":available,"orders":orders["orders"],"at":now()})
            )?
        );
        Ok(())
    }
}

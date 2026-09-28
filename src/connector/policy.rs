use super::Connector;
use crate::marketplace::{
    crypto, joyid,
    model::{Order, PROTOCOL, now},
};
use anyhow::{Context, Result, ensure};

impl Connector {
    pub async fn validate_order(&self, order: &Order, accepted: bool) -> Result<()> {
        let q = &order.quote;
        ensure!(
            q.protocol == PROTOCOL && q.network == "testnet",
            "unsupported order protocol/network"
        );
        ensure!(
            q.provider_node == self.registration.node.id
                && q.provider_pubkey == self.registration.node.pubkey,
            "order targets another provider"
        );
        ensure!(
            q.fee_recipient == self.config.owner_address,
            "fee recipient differs from local policy"
        );
        ensure!(
            q.order_id == order.id && q.merchant_account == order.owner,
            "order identity mismatch"
        );
        ensure!(
            q.expires_at > now() && q.expires_at <= now() + 900,
            "order expired or outside allowed validity window"
        );
        ensure!(
            (100..=10_000_000).contains(&q.capacity_ckb)
                && q.capacity_ckb
                    .checked_add(q.provider_reserve_ckb)
                    .and_then(|n| n.checked_add(1))
                    == Some(q.funding_ckb),
            "invalid quoted funding calculation"
        );
        ensure!(
            q.provider_reserve_ckb == 99,
            "unsupported provider reserve; re-evaluate pinned Fiber policy"
        );
        ensure!(
            q.funding_ckb <= self.config.max_order_ckb,
            "order exceeds local funding cap"
        );
        ensure!(
            q.opening_fee_ckb >= self.config.min_fee_ckb,
            "opening fee below local policy"
        );
        ensure!(
            self.config.accept_public_orders
                || self.config.allowed_merchants.contains(&q.merchant_account),
            "this provider is restricted to its locally allowed merchants"
        );
        crate::marketplace::nodes::validate_node(&q.merchant_pubkey, &q.merchant_address)?;
        let message = crypto::quote_message(q)?;
        ensure!(
            crypto::digest(message.as_bytes()) == order.quote_hash,
            "quote hash mismatch"
        );
        if accepted {
            crypto::verify(
                &q.provider_pubkey,
                &message,
                order
                    .provider_signature
                    .as_deref()
                    .context("provider signature missing")?,
            )?;
            let proof: joyid::Proof = serde_json::from_value(
                order
                    .merchant_proof
                    .clone()
                    .context("merchant signature missing")?,
            )?;
            ensure!(
                proof.address == q.merchant_account,
                "merchant approval address mismatch"
            );
            joyid::verify(&self.client, &proof, &order.approval_message()).await?;
        }
        Ok(())
    }
}

use super::error::ensure;
use super::{
    Market, auth, crypto, db,
    error::Error,
    joyid::{self, Proof},
    model::{Node, Offer, Order, PROTOCOL, Quote, now, token},
};
use anyhow::Result;
use axum::{
    Json,
    extract::{Path, State},
    http::HeaderMap,
};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
pub struct Create {
    pub offer_id: String,
    pub merchant_node: String,
    pub capacity_ckb: u64,
    pub idempotency_key: String,
}
pub async fn create(
    State(state): State<Market>,
    headers: HeaderMap,
    Json(input): Json<Create>,
) -> Result<Json<Order>, Error> {
    let owner = auth::account(&state, &headers)?;
    ensure!(
        input.idempotency_key.len() >= 16 && input.idempotency_key.len() <= 100,
        "idempotency key required"
    );
    let order = state.db.transaction(|tx| {
        let existing = db::list::<Order>(tx, "order")?;
        let request_id = crypto::digest(format!("{owner}:{}", input.idempotency_key).as_bytes());
        if let Some(order) = existing.iter().find(|o| o.id == request_id) {
            ensure!(
                order.offer_id == input.offer_id
                    && order.quote.merchant_node == input.merchant_node
                    && order.quote.capacity_ckb == input.capacity_ckb,
                "idempotency key reused for different parameters"
            );
            return Ok(order.clone());
        }
        let offer: Offer = db::get(tx, "offer", &input.offer_id)?;
        ensure!(
            offer.enabled && offer.expires_at > now(),
            "offer is no longer available"
        );
        ensure!(
            input.capacity_ckb >= offer.min_capacity_ckb
                && input.capacity_ckb <= offer.max_capacity_ckb,
            "capacity is outside offer bounds"
        );
        let merchant: Node = db::get(tx, "node", &input.merchant_node)?;
        let provider: Node = db::get(tx, "node", &offer.provider_node)?;
        ensure!(
            merchant.owner == owner && merchant.role == "merchant",
            "merchant node ownership required"
        );
        ensure!(
            provider.owner != owner && provider.pubkey != merchant.pubkey,
            "provider and merchant must be separate participants"
        );
        ensure!(
            now() - provider.last_seen <= 90 && now() - merchant.last_seen <= 90,
            "both node connectors must be online"
        );
        let outstanding = existing.iter().any(|o| {
            o.owner == owner
                && o.quote.provider_node == provider.id
                && (o.reserves_funds()
                    || (o.status == "delivered"
                        && !matches!(o.fee_status.as_str(), "paid" | "waived")))
        });
        ensure!(
            !outstanding,
            "complete or cancel your outstanding provider order first"
        );
        let funding = input.capacity_ckb + provider.reserve_ckb + 1;
        if let Some(policy) = &provider.provider_policy {
            ensure!(
                offer.opening_fee_ckb >= policy.min_fee_ckb,
                "offer fee is below the provider's current minimum"
            );
        }
        let reserved = existing
            .iter()
            .filter(|o| o.quote.provider_node == provider.id && o.reserves_funds())
            .map(|o| o.quote.funding_ckb)
            .sum::<u64>();
        ensure!(
            provider.available_funding(reserved) >= funding,
            "provider has insufficient unreserved funding within its limits"
        );
        ensure!(
            merchant.available_ckb >= merchant.reserve_ckb + 63,
            "receiving node needs CKB for its reserve, change, and fees"
        );
        let automatic = provider.automatic();
        let quote = Quote {
            protocol: PROTOCOL.into(),
            network: "testnet".into(),
            order_id: request_id.clone(),
            provider_node: provider.id,
            provider_pubkey: provider.pubkey,
            merchant_node: merchant.id,
            merchant_pubkey: merchant.pubkey,
            merchant_address: merchant.address,
            merchant_account: owner.clone(),
            capacity_ckb: input.capacity_ckb,
            funding_ckb: funding,
            provider_reserve_ckb: provider.reserve_ckb,
            merchant_reserve_ckb: merchant.reserve_ckb,
            opening_fee_ckb: offer.opening_fee_ckb,
            fee_recipient: offer.owner,
            public_channel: offer.public_channel,
            expires_at: now() + 600,
            nonce: token(),
        };
        let hash = crypto::digest(crypto::quote_message(&quote)?.as_bytes());
        let order = Order {
            id: request_id,
            owner: owner.clone(),
            offer_id: offer.id,
            automatic,
            quote,
            quote_hash: hash,
            provider_signature: None,
            merchant_proof: None,
            status: "awaiting_quote".into(),
            channel_id: None,
            funding_outpoint: None,
            provider_evidence: None,
            merchant_evidence: None,
            probe_invoice: None,
            probe_payment_hash: None,
            probe_received: false,
            delivery_block_number: None,
            delivered_at: None,
            fee_status: "not_due".into(),
            fee_tx_hash: None,
            error: None,
            created_at: now(),
            updated_at: now(),
        };
        db::put(tx, "order", &order.id, &owner, &order)?;
        db::event(tx, &order.id, &owner, "order_created")?;
        Ok(order)
    })?;
    Ok(Json(order))
}

#[derive(Deserialize)]
pub struct Accept {
    pub proof: Proof,
}
pub async fn accept(
    State(state): State<Market>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<Accept>,
) -> Result<Json<Order>, Error> {
    let owner = auth::account(&state, &headers)?;
    let order: Order = state.db.get("order", &id)?;
    ensure!(
        order.owner == owner && input.proof.address == owner,
        "order belongs to another wallet"
    );
    ensure!(
        order.status == "quoted" && order.quote.expires_at > now(),
        "quote unavailable or expired"
    );
    joyid::verify(&state.client, &input.proof, &order.approval_message()).await?;
    let updated = state.db.transaction(|tx| {
        let mut current: Order = db::get(tx, "order", &id)?;
        ensure!(
            current.status == "quoted"
                && current.quote_hash == order.quote_hash
                && current.quote.expires_at > now(),
            "quote changed or expired"
        );
        current.merchant_proof = Some(serde_json::to_value(input.proof)?);
        current.status = "accepted".into();
        current.updated_at = now();
        db::put(tx, "order", &id, &owner, &current)?;
        db::event(tx, &id, &owner, "merchant_accepted")?;
        Ok(current)
    })?;
    Ok(Json(updated))
}

pub async fn cancel(
    State(state): State<Market>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Order>, Error> {
    let owner = auth::account(&state, &headers)?;
    let order = state.db.transaction(|tx| {
        let mut order: Order = db::get(tx, "order", &id)?;
        ensure!(order.owner == owner, "order belongs to another wallet");
        ensure!(
            matches!(
                order.status.as_str(),
                "awaiting_quote" | "quoted" | "accepted"
            ),
            "funding may have started; reconcile with the provider before cancellation"
        );
        order.status = "cancelled".into();
        order.updated_at = now();
        db::put(tx, "order", &id, &owner, &order)?;
        db::event(tx, &id, &owner, "cancelled")?;
        Ok(order)
    })?;
    Ok(Json(order))
}

pub async fn dashboard(
    State(state): State<Market>,
    headers: HeaderMap,
) -> Result<Json<Value>, Error> {
    let owner = auth::account(&state, &headers)?;
    let nodes: Vec<Node> = state
        .db
        .list::<Node>("node")?
        .into_iter()
        .filter(|n| n.owner == owner)
        .collect();
    let orders: Vec<Order> = state
        .db
        .list::<Order>("order")?
        .into_iter()
        .filter(|o| o.owner == owner || o.quote.fee_recipient == owner)
        .collect();
    let offers: Vec<Offer> = state
        .db
        .list::<Offer>("offer")?
        .into_iter()
        .filter(|o| o.owner == owner)
        .collect();
    Ok(Json(
        json!({"address":owner,"nodes":nodes,"orders":orders,"offers":offers}),
    ))
}

pub fn expire(state: &Market) -> Result<()> {
    state.db.transaction(|tx| {
        for mut order in db::list::<Order>(tx, "order")? {
            if order.fee_status == "due" && order.delivered_at.is_some_and(|at| now() - at > 86400)
            {
                order.fee_status = "overdue".into();
                db::put(tx, "order", &order.id, &order.owner, &order)?;
                db::event(tx, &order.id, "system", "fee_overdue")?;
            }
            if order.quote.expires_at < now()
                && matches!(
                    order.status.as_str(),
                    "awaiting_quote" | "quoted" | "accepted"
                )
            {
                order.status = "expired".into();
                order.updated_at = now();
                db::put(tx, "order", &order.id, &order.owner, &order)?;
                db::event(tx, &order.id, "system", "quote_expired")?;
            }
        }
        Ok(())
    })
}

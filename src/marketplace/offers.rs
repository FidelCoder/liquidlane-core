use super::error::ensure;

use super::{
    Market, auth, crypto, db,
    error::Error,
    model::{Node, Offer, Order, id, now},
};
use axum::{
    Json,
    extract::{Path, State},
    http::HeaderMap,
};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
pub struct Input {
    pub provider_node: String,
    pub min_capacity_ckb: u64,
    pub max_capacity_ckb: u64,
    pub opening_fee_ckb: u64,
    pub public_channel: bool,
}
pub async fn create(
    State(state): State<Market>,
    headers: HeaderMap,
    Json(input): Json<Input>,
) -> Result<Json<Offer>, Error> {
    let owner = auth::account(&state, &headers)?;
    let node: Node = state.db.get("node", &input.provider_node)?;
    ensure!(
        node.owner == owner && node.role == "provider",
        "provider node ownership required"
    );
    ensure!(
        input.min_capacity_ckb >= 100
            && input.max_capacity_ckb >= input.min_capacity_ckb
            && input.max_capacity_ckb <= 10_000_000,
        "capacity must be between 100 and 10,000,000 CKB"
    );
    ensure!(
        input.opening_fee_ckb >= crypto::minimum_cell_ckb(&owner)?
            && input.opening_fee_ckb <= input.max_capacity_ckb,
        "opening fee must cover a direct recipient cell and not exceed maximum capacity"
    );
    if let Some(policy) = &node.provider_policy {
        ensure!(
            input.max_capacity_ckb + node.reserve_ckb + 1 <= policy.max_order_ckb,
            "offer capacity exceeds the node's per-order funding limit"
        );
        ensure!(
            input.opening_fee_ckb >= policy.min_fee_ckb,
            "offer fee is below the node's minimum accepted fee"
        );
    }
    let offer = Offer {
        id: id(),
        owner,
        provider_node: input.provider_node,
        min_capacity_ckb: input.min_capacity_ckb,
        max_capacity_ckb: input.max_capacity_ckb,
        opening_fee_ckb: input.opening_fee_ckb,
        public_channel: input.public_channel,
        enabled: true,
        created_at: now(),
        expires_at: now() + 86400,
    };
    state.db.transaction(|tx| {
        let node: Node = db::get(tx, "node", &offer.provider_node)?;
        ensure!(now().abs_diff(node.last_seen) <= 90 && node.reserve_ckb >= 99, "connect your provider node before publishing");
        let reserved = db::list::<Order>(tx, "order")?.iter().filter(|o| o.quote.provider_node == node.id && o.reserves_funds()).map(|o| o.quote.funding_ckb).sum();
        ensure!(node.available_funding(reserved) >= offer.min_capacity_ckb + node.reserve_ckb + 1, "add capital to your node or reduce the offer minimum before publishing; funding must fit within your local budget");
        for mut old in db::list::<Offer>(tx, "offer")?
            .into_iter()
            .filter(|o| o.provider_node == offer.provider_node)
        {
            old.enabled = false;
            db::put(tx, "offer", &old.id, &old.owner, &old)?;
        }
        db::put(tx, "offer", &offer.id, &offer.owner, &offer)
    })?;
    Ok(Json(offer))
}
pub async fn list(State(state): State<Market>) -> Result<Json<Value>, Error> {
    let offers = state.db.list::<Offer>("offer")?;
    let nodes = state.db.list::<Node>("node")?;
    let orders = state.db.list::<Order>("order")?;
    let offers:Vec<_>=offers.into_iter().filter(|o|o.enabled && o.expires_at>now()).filter_map(|offer|{
        let node=nodes.iter().find(|n|n.id==offer.provider_node)?;
        let reserved=orders.iter().filter(|o|o.quote.provider_node==node.id&&o.reserves_funds()).map(|o|o.quote.funding_ckb).sum::<u64>();
        let fee_allowed=node.provider_policy.as_ref().is_none_or(|p| offer.opening_fee_ckb >= p.min_fee_ckb);
        let available=if fee_allowed { node.available_funding(reserved).saturating_sub(node.reserve_ckb+1).min(offer.max_capacity_ckb) } else { 0 };
        Some(json!({"offer":offer,"provider":{"id":node.id,"label":node.label,"pubkey":node.pubkey,"last_seen":node.last_seen,"provider_policy":node.provider_policy},"automatic":node.automatic(),"online":now()-node.last_seen<=90,"available_capacity_ckb":available}))
    }).collect();
    Ok(Json(
        json!({"offers":offers,"network":"testnet","service":"initial_receive_capacity","platform_fee_ckb":0}),
    ))
}
pub async fn disable(
    State(state): State<Market>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Offer>, Error> {
    let owner = auth::account(&state, &headers)?;
    let offer = state.db.transaction(|tx| {
        let mut offer: Offer = db::get(tx, "offer", &id)?;
        ensure!(offer.owner == owner, "provider ownership required");
        offer.enabled = false;
        db::put(tx, "offer", &id, &owner, &offer)?;
        Ok(offer)
    })?;
    Ok(Json(offer))
}

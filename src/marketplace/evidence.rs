use super::error::ensure;
use super::{
    Market, auth, chain, db,
    error::Error,
    model::{ChannelEvidence, Node, Order, SHANNONS, now},
    nodes::{SignedPayload, check_payload},
};
use anyhow::Context;
use axum::{
    Json,
    extract::{Path, State},
    http::HeaderMap,
};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct Report {
    pub order_id: String,
    pub evidence: ChannelEvidence,
}
pub async fn report(
    State(state): State<Market>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<SignedPayload>,
) -> Result<Json<Order>, Error> {
    let node_id = auth::node(&state, &headers)?;
    let node: Node = state.db.get("node", &node_id)?;
    let mut report: Report = check_payload(&node, &input)?;
    report.evidence.funding_outpoint =
        chain::canonical_outpoint(&report.evidence.funding_outpoint)?;
    let channel_bytes = super::crypto::bytes(&report.evidence.channel_id)?;
    ensure!(channel_bytes.len() == 32, "invalid channel ID");
    report.evidence.channel_id = format!("0x{}", hex::encode(channel_bytes));
    ensure!(
        report.order_id == id && now().abs_diff(report.evidence.observed_at) <= 60,
        "stale or mismatched channel evidence"
    );
    let order: Order = state.db.get("order", &id)?;
    let provider = order.quote.provider_node == node_id;
    ensure!(
        provider || order.quote.merchant_node == node_id,
        "node does not participate in this order"
    );
    ensure!(
        &report.evidence.peer_pubkey
            == if provider {
                &order.quote.merchant_pubkey
            } else {
                &order.quote.provider_pubkey
            },
        "channel peer does not match the signed quote"
    );
    ensure!(
        !matches!(
            order.status.as_str(),
            "awaiting_quote" | "quoted" | "accepted" | "cancelled" | "expired" | "failed"
        ),
        "provider must claim opening before reporting a channel"
    );
    chain::verify_funding(&state, &order, &report.evidence.funding_outpoint).await?;
    if let Some(proof) = &report.evidence.settlement {
        report.evidence.settlement = Some(
            tokio::time::timeout(
                std::time::Duration::from_secs(25),
                super::settlement::verify(
                    &state.client,
                    &state.config.ckb_rpc,
                    &report.evidence.funding_outpoint,
                    proof,
                ),
            )
            .await
            .context("settlement verification timed out")??,
        );
    }
    let updated = state.db.transaction(|tx| {
        let mut order: Order = db::get(tx, "order", &id)?;
        ensure!(!matches!(order.status.as_str(), "failed" | "cancelled" | "expired"),
            "order is already terminal");
        db::bind_outpoint(tx, &report.evidence.funding_outpoint, &id)?;
        if let Some(existing) = &order.channel_id {
            ensure!(
                existing == &report.evidence.channel_id,
                "channel identity cannot change"
            );
        }
        if let Some(existing) = &order.funding_outpoint {
            ensure!(
                chain::canonical_outpoint(existing)? == report.evidence.funding_outpoint,
                "funding outpoint cannot change"
            );
        }
        order.channel_id = Some(report.evidence.channel_id.clone());
        order.funding_outpoint = Some(report.evidence.funding_outpoint.clone());
        if provider {
            order.provider_evidence = Some(report.evidence);
        } else {
            order.merchant_evidence = Some(report.evidence);
        }
        if order.status != "delivered" {
            let peers = [&order.provider_evidence, &order.merchant_evidence];
            let settled = peers.iter().filter_map(|e| e.as_ref()).any(|e| e.settlement.as_ref().is_some_and(|s| s.confirmed));
            let closing = peers.iter().filter_map(|e| e.as_ref()).any(|e| matches!(e.state.as_str(), "Closed" | "ShuttingDown") || e.settlement.is_some());
            order.status = if settled { "failed" } else if closing { "reconciling" } else { "verifying" }.into();
            order.error = if settled {
                Some("Channel settled before verified delivery. No opening fee is due.".into())
            } else if closing {
                Some("Channel closing before verified delivery; waiting for confirmed native settlement. No opening fee is due.".into())
            } else { None };
        } else {order.error=None;}
        order.updated_at = now();
        db::put(
            tx,
            "attestation",
            &format!("{id}:{node_id}"),
            &node.owner,
            &input,
        )?;
        db::put(tx, "order", &id, &order.owner, &order)?;
        Ok(order)
    })?;
    Ok(Json(updated))
}

pub async fn verify_delivery(
    State(state): State<Market>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Order>, Error> {
    let owner = auth::account(&state, &headers)?;
    let order: Order = state.db.get("order", &id)?;
    ensure!(order.owner == owner, "merchant ownership required");
    if order.status == "delivered" {
        return Ok(Json(order));
    }
    ready(&order)?;
    chain::verify_funding(
        &state,
        &order,
        order
            .funding_outpoint
            .as_deref()
            .context("funding not yet verified")?,
    )
    .await?;
    chain::verify_live_funding(
        &state,
        order
            .funding_outpoint
            .as_deref()
            .context("funding not yet verified")?,
    )
    .await?;
    let delivery_tip = chain::number(
        &chain::rpc(
            &state.client,
            &state.config.ckb_rpc,
            "get_tip_block_number",
            serde_json::json!([]),
        )
        .await?,
    )?;
    let updated = state.db.transaction(|tx| {
        let mut current: Order = db::get(tx, "order", &id)?;
        ready(&current)?;
        current.status = "delivered".into();
        current.fee_status = "due".into();
        current.updated_at = now();
        current.delivery_block_number = Some(delivery_tip);
        current.delivered_at = Some(now());
        super::receipts::save(tx, &current)?;
        db::put(tx, "order", &id, &owner, &current)?;
        db::event(tx, &id, &owner, "delivery_verified")?;
        Ok(current)
    })?;
    Ok(Json(updated))
}

pub fn ready(order: &Order) -> anyhow::Result<()> {
    ensure!(
        order.status == "verifying",
        "order is not ready for delivery verification"
    );
    let p = order
        .provider_evidence
        .as_ref()
        .context("provider observation required")?;
    let m = order
        .merchant_evidence
        .as_ref()
        .context("merchant observation required")?;
    ensure!(
        now().abs_diff(p.observed_at) <= 90 && now().abs_diff(m.observed_at) <= 90,
        "channel observations expired"
    );
    ensure!(
        p.state == "ChannelReady"
            && m.state == "ChannelReady"
            && p.settlement.is_none()
            && m.settlement.is_none(),
        "both peers must report ChannelReady"
    );
    ensure!(
        p.channel_id == m.channel_id && p.funding_outpoint == m.funding_outpoint,
        "peer channel observations disagree"
    );
    ensure!(
        p.local_balance == m.remote_balance && p.remote_balance == m.local_balance,
        "peer balance observations disagree; wait for both connectors to refresh"
    );
    let inbound = chain::number(&serde_json::Value::String(m.inbound_liquidity.clone()))?;
    ensure!(
        inbound
            >= order
                .quote
                .capacity_ckb
                .checked_mul(SHANNONS)
                .context("capacity overflow")?,
        "merchant inbound capacity is below the signed quote"
    );
    ensure!(
        order.probe_received,
        "receiving node must confirm a real delivery-probe payment"
    );
    Ok(())
}

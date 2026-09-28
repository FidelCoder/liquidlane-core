use super::error::ensure;
use super::{
    Market, auth, crypto, db,
    error::Error,
    model::{Node, Order, now},
    nodes::{SignedPayload, check_payload},
};
use anyhow::Context;
use axum::{
    Json,
    extract::{Path, State},
    http::HeaderMap,
};
use serde::Deserialize;
use serde_json::{Value, json};

pub async fn orders(State(state): State<Market>, headers: HeaderMap) -> Result<Json<Value>, Error> {
    let node = auth::node(&state, &headers)?;
    let orders: Vec<Order> = state
        .db
        .list::<Order>("order")?
        .into_iter()
        .filter(|o| o.quote.provider_node == node || o.quote.merchant_node == node)
        .collect();
    Ok(Json(json!({"orders":orders})))
}
#[derive(Deserialize)]
pub struct QuoteSignature {
    pub signature: String,
}
pub async fn quote(
    State(state): State<Market>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<QuoteSignature>,
) -> Result<Json<Order>, Error> {
    let node_id = auth::node(&state, &headers)?;
    let order = state.db.transaction(|tx| {
        let mut order: Order = db::get(tx, "order", &id)?;
        ensure!(
            order.quote.provider_node == node_id,
            "provider ownership required"
        );
        ensure!(
            matches!(order.status.as_str(), "awaiting_quote" | "quoted")
                && order.quote.expires_at > now(),
            "quote expired or no longer pending"
        );
        crypto::verify(
            &order.quote.provider_pubkey,
            &crypto::quote_message(&order.quote)?,
            &input.signature,
        )?;
        order.provider_signature = Some(input.signature);
        order.status = "quoted".into();
        order.updated_at = now();
        db::put(tx, "order", &id, &order.owner, &order)?;
        db::event(tx, &id, &node_id, "provider_quoted")?;
        Ok(order)
    })?;
    Ok(Json(order))
}
#[derive(Deserialize)]
pub struct Start {
    pub order_id: String,
    pub action: String,
}
pub async fn start(
    State(state): State<Market>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<SignedPayload>,
) -> Result<Json<Order>, Error> {
    let node_id = auth::node(&state, &headers)?;
    let node: Node = state.db.get("node", &node_id)?;
    let signed: Start = check_payload(&node, &input)?;
    ensure!(
        signed.order_id == id && signed.action == "start",
        "start signature does not match order"
    );
    let order = state.db.transaction(|tx| {
        let mut order: Order = db::get(tx, "order", &id)?;
        ensure!(
            order.quote.provider_node == node_id,
            "provider ownership required"
        );
        ensure!(
            order.status == "accepted" && order.quote.expires_at > now(),
            "order is not available for opening"
        );
        ensure!(
            order.merchant_proof.is_some() && order.provider_signature.is_some(),
            "both participants must authorize the order"
        );
        order.status = "opening".into();
        order.updated_at = now();
        db::put(tx, "order", &id, &order.owner, &order)?;
        db::event(
            tx,
            &id,
            &node_id,
            if order.automatic {
                "automatic_provider_authorization"
            } else {
                "local_provider_approval"
            },
        )?;
        Ok(order)
    })?;
    Ok(Json(order))
}
#[derive(Deserialize)]
pub struct Failure {
    pub order_id: String,
    pub error: String,
    pub before_funding: bool,
}
pub async fn failure(
    State(state): State<Market>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<SignedPayload>,
) -> Result<Json<Order>, Error> {
    let node_id = auth::node(&state, &headers)?;
    let node: Node = state.db.get("node", &node_id)?;
    let report: Failure = check_payload(&node, &input)?;
    ensure!(
        report.order_id == id && report.error.len() <= 500,
        "invalid failure report"
    );
    let order = state.db.transaction(|tx| {
        let mut order: Order = db::get(tx, "order", &id)?;
        ensure!(
            order.quote.provider_node == node_id,
            "provider ownership required"
        );
        ensure!(
            matches!(
                order.status.as_str(),
                "awaiting_quote"
                    | "quoted"
                    | "accepted"
                    | "opening"
                    | "awaiting_confirmation"
                    | "verifying"
                    | "reconciling"
            ),
            "terminal orders cannot be failed"
        );
        if report.before_funding
            && matches!(
                order.status.as_str(),
                "awaiting_quote" | "quoted" | "accepted"
            )
            && order.funding_outpoint.is_none()
            && order.channel_id.is_none()
        {
            order.status = "failed".into();
        } else {
            order.status = "reconciling".into();
        }
        order.error = Some(report.error);
        order.updated_at = now();
        db::put(tx, "order", &id, &order.owner, &order)?;
        db::event(tx, &id, &node_id, "provider_failure")?;
        Ok(order)
    })?;
    Ok(Json(order))
}
#[derive(Deserialize)]
pub struct Probe {
    pub order_id: String,
    pub invoice: Option<String>,
    pub payment_hash: String,
    pub paid: bool,
}
pub async fn probe(
    State(state): State<Market>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<SignedPayload>,
) -> Result<Json<Order>, Error> {
    let node_id = auth::node(&state, &headers)?;
    let node: Node = state.db.get("node", &node_id)?;
    let report: Probe = check_payload(&node, &input)?;
    ensure!(report.order_id == id, "probe does not match order");
    ensure!(
        crypto::bytes(&report.payment_hash)?.len() == 32,
        "invalid payment hash"
    );
    let order = state.db.transaction(|tx| {
        let mut order: Order = db::get(tx, "order", &id)?;
        ensure!(
            order.quote.merchant_node == node_id,
            "merchant node ownership required"
        );
        ensure!(
            order.merchant_evidence.is_some(),
            "observe the receiving channel before creating a delivery probe"
        );
        if let Some(existing) = &order.probe_payment_hash {
            ensure!(
                existing == &report.payment_hash,
                "delivery probe cannot be replaced"
            );
        } else {
            let invoice = report
                .invoice
                .as_deref()
                .context("probe invoice required")?;
            ensure!(
                invoice.starts_with("fibt") && invoice.len() < 4000,
                "CKB testnet Fiber invoice required"
            );
            order.probe_invoice = Some(invoice.into());
            order.probe_payment_hash = Some(report.payment_hash);
        }
        if report.paid {
            order.probe_received = true;
        }
        db::put(tx, "probe_report", &id, &node.owner, &input)?;
        order.updated_at = now();
        db::put(tx, "order", &id, &order.owner, &order)?;
        db::event(
            tx,
            &id,
            &node_id,
            if report.paid {
                "merchant_received_probe"
            } else {
                "merchant_created_probe"
            },
        )?;
        Ok(order)
    })?;
    Ok(Json(order))
}

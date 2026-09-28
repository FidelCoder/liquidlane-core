use super::{
    Market, auth, db,
    error::{Error, ensure},
    model::Order,
};
use axum::{
    Json,
    extract::{Path, State},
    http::HeaderMap,
};
use rusqlite::Transaction;
use serde_json::{Value, json};

pub fn save(tx: &Transaction<'_>, order: &Order) -> anyhow::Result<()> {
    let provider: Value = db::get(
        tx,
        "attestation",
        &format!("{}:{}", order.id, order.quote.provider_node),
    )?;
    let merchant: Value = db::get(
        tx,
        "attestation",
        &format!("{}:{}", order.id, order.quote.merchant_node),
    )?;
    let probe: Value = db::get(tx, "probe_report", &order.id)?;
    db::put(
        tx,
        "delivery_receipt",
        &order.id,
        &order.owner,
        &json!({"order_at_delivery":order,"provider_attestation":provider,"merchant_attestation":merchant,"merchant_probe_attestation":probe,"trust":"Signed participant observations plus confirmed native funding; no proof of future availability."}),
    )
}
pub async fn get(
    State(state): State<Market>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Error> {
    let owner = auth::account(&state, &headers)?;
    let order: Order = state.db.get("order", &id)?;
    ensure!(
        order.owner == owner || order.quote.fee_recipient == owner,
        "participant ownership required"
    );
    let receipt: Value = state.db.get("delivery_receipt", &id)?;
    let events=state.db.transaction(|tx|{
        let mut query=tx.prepare("SELECT actor,action,at FROM events WHERE order_id=?1 ORDER BY seq")?;
        let events=query.query_map([&id],|r|Ok(json!({"actor":r.get::<_,String>(0)?,"action":r.get::<_,String>(1)?,"at":r.get::<_,i64>(2)?})))?;
        Ok(events.collect::<Result<Vec<_>,_>>()?)
    })?;
    Ok(Json(
        json!({"delivery":receipt,"current_order":order,"events":events}),
    ))
}

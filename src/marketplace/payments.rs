use super::error::ensure;

use super::{
    Market, auth, chain, crypto, db,
    error::Error,
    model::{Order, now},
};
use axum::{
    Json,
    extract::{Path, State},
    http::HeaderMap,
};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct Payment {
    pub tx_hash: String,
}
pub async fn settle(
    State(state): State<Market>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(mut input): Json<Payment>,
) -> Result<Json<Order>, Error> {
    let owner = auth::account(&state, &headers)?;
    let order: Order = state.db.get("order", &id)?;
    ensure!(
        order.owner == owner && order.status == "delivered",
        "only a delivered order can receive a fee payment"
    );
    ensure!(
        crypto::bytes(&input.tx_hash)?.len() == 32,
        "invalid fee transaction hash"
    );
    input.tx_hash = format!("0x{}", hex::encode(crypto::bytes(&input.tx_hash)?));
    if order.fee_status == "paid" {
        ensure!(
            order.fee_tx_hash.as_deref() == Some(&input.tx_hash),
            "order already paid with another transaction"
        );
        return Ok(Json(order));
    }
    chain::verify_fee(&state, &order, &input.tx_hash).await?;
    let updated = state.db.transaction(|tx| {
        let mut order: Order = db::get(tx, "order", &id)?;
        db::bind(tx, "fee_transaction", &input.tx_hash, &id)?;
        ensure!(
            order
                .fee_tx_hash
                .as_ref()
                .is_none_or(|hash| hash == &input.tx_hash),
            "order already has another fee payment"
        );
        order.fee_status = "paid".into();
        order.fee_tx_hash = Some(input.tx_hash);
        order.updated_at = now();
        db::put(tx, "order", &id, &owner, &order)?;
        db::event(tx, &id, &owner, "confirmed_fee_payment")?;
        Ok(order)
    })?;
    Ok(Json(updated))
}
pub async fn waive(
    State(state): State<Market>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Order>, Error> {
    let owner = auth::account(&state, &headers)?;
    let order = state.db.transaction(|tx| {
        let mut order: Order = db::get(tx, "order", &id)?;
        ensure!(
            order.quote.fee_recipient == owner && order.status == "delivered",
            "provider ownership and delivered service required"
        );
        ensure!(
            order.fee_status != "paid",
            "confirmed payments cannot be waived retroactively"
        );
        order.fee_status = "waived".into();
        order.updated_at = now();
        db::put(tx, "order", &id, &order.owner, &order)?;
        db::event(tx, &id, &owner, "fee_waived")?;
        Ok(order)
    })?;
    Ok(Json(order))
}

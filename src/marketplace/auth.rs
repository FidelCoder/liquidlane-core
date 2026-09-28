use super::error::ensure;
use super::{
    Market, crypto, db,
    error::Error,
    joyid::{self, Proof},
    model::{Account, id, now, token},
};
use anyhow::{Context, Result};
use axum::{Json, extract::State, http::HeaderMap};
use rusqlite::{OptionalExtension, params};
use serde::Deserialize;
use serde_json::{Value, json};

pub fn bearer(headers: &HeaderMap) -> Result<&str> {
    headers
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
        .context("unauthorized: bearer token required")
}
pub fn account(state: &Market, headers: &HeaderMap) -> Result<String> {
    state.db.session(bearer(headers)?, "account")
}
pub fn node(state: &Market, headers: &HeaderMap) -> Result<String> {
    state.db.session(bearer(headers)?, "node")
}

#[derive(Deserialize)]
pub struct ChallengeRequest {
    pub address: String,
}
pub async fn challenge(
    State(state): State<Market>,
    Json(input): Json<ChallengeRequest>,
) -> Result<Json<Value>, Error> {
    crypto::address_script(&input.address)?;
    let challenge_id = id();
    let expires = now() + 300;
    let message = format!(
        "LiquidLane wallet session\nOrigin: {}\nNetwork: testnet\nAddress: {}\nChallenge: {}\nNonce: {}\nExpires: {}\nThis signs you in. It does not authorize a transaction or channel.",
        state.config.origin,
        input.address,
        challenge_id,
        token(),
        expires
    );
    state.db.transaction(|tx| {
        tx.execute("DELETE FROM challenges WHERE expires<?1", [now()])?;
        tx.execute("DELETE FROM sessions WHERE expires<?1", [now()])?;
        let count: i64 = tx.query_row(
            "SELECT count(*) FROM challenges WHERE address=?1",
            [&input.address],
            |r| r.get(0),
        )?;
        ensure!(
            count < 10,
            "too many pending challenges; try again after five minutes"
        );
        tx.execute(
            "INSERT INTO challenges VALUES(?1,?2,?3,?4)",
            params![challenge_id, input.address, message, expires],
        )?;
        Ok(())
    })?;
    Ok(Json(
        json!({"challenge_id":challenge_id,"message":message,"expires_at":expires}),
    ))
}

#[derive(Deserialize)]
pub struct VerifyRequest {
    pub challenge_id: String,
    pub proof: Proof,
}
pub async fn verify(
    State(state): State<Market>,
    Json(input): Json<VerifyRequest>,
) -> Result<Json<Value>, Error> {
    let message: String = state.db.transaction(|tx| {
        tx.query_row(
            "SELECT message FROM challenges WHERE id=?1 AND address=?2 AND expires>?3",
            params![input.challenge_id, input.proof.address, now()],
            |r| r.get(0),
        )
        .optional()?
        .context("unauthorized: challenge expired, consumed, or address mismatch")
    })?;
    joyid::verify(&state.client, &input.proof, &message).await?;
    let session = token();
    let expires = now() + 12 * 3600;
    state.db.transaction(|tx| {
        ensure!(
            tx.execute(
                "DELETE FROM challenges WHERE id=?1 AND expires>?2",
                params![input.challenge_id, now()]
            )? == 1,
            "unauthorized: challenge already consumed"
        );
        let account = Account {
            address: input.proof.address.clone(),
            created_at: now(),
        };
        db::put(tx, "account", &account.address, &account.address, &account)?;
        tx.execute(
            "INSERT INTO sessions VALUES(?1,?2,'account',?3)",
            params![crypto::digest(session.as_bytes()), account.address, expires],
        )?;
        Ok(())
    })?;
    Ok(Json(
        json!({"token":session,"address":input.proof.address,"expires_at":expires}),
    ))
}

pub async fn logout(State(state): State<Market>, headers: HeaderMap) -> Result<Json<Value>, Error> {
    let hash = crypto::digest(bearer(&headers)?.as_bytes());
    state.db.transaction(|tx| {
        tx.execute("DELETE FROM sessions WHERE hash=?1", [hash])?;
        Ok(())
    })?;
    Ok(Json(json!({"signed_out":true})))
}

use super::error::ensure;
pub use super::heartbeat::heartbeat;
use super::{
    Market, auth, crypto, db,
    error::Error,
    model::{Node, TESTNET_GENESIS, id, now, token},
};
use anyhow::{Context, Result};
use axum::{
    Json,
    extract::{Path, State},
    http::HeaderMap,
};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Serialize, Deserialize)]
struct Pairing {
    owner: String,
    role: String,
    label: String,
    expires_at: i64,
}
#[derive(Serialize, Deserialize)]
struct PairingStatus {
    owner: String,
    expires_at: i64,
    node_id: Option<String>,
}
#[derive(Deserialize)]
pub struct PairInput {
    pub role: String,
    pub label: String,
}
pub async fn pair(
    State(state): State<Market>,
    headers: HeaderMap,
    Json(input): Json<PairInput>,
) -> Result<Json<Value>, Error> {
    let owner = auth::account(&state, &headers)?;
    ensure!(
        matches!(input.role.as_str(), "provider" | "merchant"),
        "invalid node role"
    );
    ensure!(
        !input.label.trim().is_empty() && input.label.len() <= 80,
        "node label must contain 1–80 characters"
    );
    let code = token();
    let pairing_id = crypto::digest(code.as_bytes());
    let expires = now() + 600;
    state.db.transaction(|tx| {
        db::put(
            tx,
            "pairing",
            &pairing_id,
            &owner,
            &Pairing {
                owner: owner.clone(),
                role: input.role.clone(),
                label: input.label.clone(),
                expires_at: expires,
            },
        )?;
        db::put(
            tx,
            "pairing_status",
            &pairing_id,
            &owner,
            &PairingStatus {
                owner: owner.clone(),
                expires_at: expires,
                node_id: None,
            },
        )
    })?;
    Ok(Json(
        json!({"pairing_id":pairing_id,"pairing_code":code,"expires_at":expires,"role":input.role,"account":owner,"label":input.label}),
    ))
}

pub async fn pairing_status(
    State(state): State<Market>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Error> {
    let owner = auth::account(&state, &headers)?;
    let result: PairingStatus = state.db.get("pairing_status", &id)?;
    ensure!(result.owner == owner, "pairing belongs to another wallet");
    let node = result
        .node_id
        .map(|id| state.db.get::<Node>("node", &id))
        .transpose()?;
    let status = if node.is_some() {
        "paired"
    } else if result.expires_at <= now() {
        "expired"
    } else {
        "waiting"
    };
    Ok(Json(
        json!({"status":status,"expires_at":result.expires_at,"node":node}),
    ))
}

#[derive(Serialize, Deserialize)]
pub struct Registration {
    pub pairing_code: String,
    pub pubkey: String,
    pub address: String,
    pub version: String,
    pub chain_hash: String,
    pub signature: String,
}
pub fn registration_message(code: &str, pubkey: &str, address: &str) -> String {
    format!(
        "LiquidLane node pairing\nNetwork: testnet\nPairing: {code}\nNode: {pubkey}\nAddress: {address}"
    )
}
pub async fn register(
    State(state): State<Market>,
    Json(mut input): Json<Registration>,
) -> Result<Json<Value>, Error> {
    validate_node(&input.pubkey, &input.address)?;
    ensure!(
        input.chain_hash == TESTNET_GENESIS,
        "node must use CKB testnet"
    );
    ensure!(
        input.version == state.config.fiber_version,
        "Fiber version must match the pinned marketplace version"
    );
    crypto::verify(
        &input.pubkey,
        &registration_message(&input.pairing_code, &input.pubkey, &input.address),
        &input.signature,
    )?;
    input.pubkey = hex::encode(crypto::bytes(&input.pubkey)?);
    let hash = crypto::digest(input.pairing_code.as_bytes());
    let session = token();
    let node_id = id();
    let node = state.db.transaction(|tx| {
        let pairing: Pairing = db::get(tx, "pairing", &hash)?;
        ensure!(pairing.expires_at > now(), "pairing code expired");
        let existing = db::list::<Node>(tx, "node")?
            .into_iter()
            .find(|n| n.pubkey == input.pubkey);
        let id = if let Some(existing) = existing {
            ensure!(
                existing.owner == pairing.owner && existing.role == pairing.role,
                "node already belongs to another account or role"
            );
            existing.id
        } else {
            node_id
        };
        let node = Node {
            id: id.clone(),
            owner: pairing.owner,
            role: pairing.role,
            label: pairing.label,
            pubkey: input.pubkey,
            address: input.address,
            version: input.version,
            last_seen: 0,
            available_ckb: 0,
            reserve_ckb: 0,
            provider_policy: None,
            funding: None,
            background: false,
        };
        db::put(tx, "node", &id, &node.owner, &node)?;
        db::put(
            tx,
            "pairing_status",
            &hash,
            &node.owner,
            &PairingStatus {
                owner: node.owner.clone(),
                expires_at: pairing.expires_at,
                node_id: Some(id.clone()),
            },
        )?;
        tx.execute(
            "DELETE FROM records WHERE kind='pairing' AND id=?1",
            [&hash],
        )?;
        tx.execute(
            "DELETE FROM sessions WHERE scope='node' AND actor=?1",
            [&id],
        )?;
        tx.execute(
            "INSERT INTO sessions VALUES(?1,?2,'node',?3)",
            params![crypto::digest(session.as_bytes()), id, now() + 90 * 86400],
        )?;
        Ok(node)
    })?;
    Ok(Json(
        json!({"token":session,"node":node,"expires_at":now()+90*86400}),
    ))
}

#[derive(Serialize, Deserialize)]
pub struct SignedPayload {
    pub payload: String,
    pub signature: String,
}
pub fn check_payload<T: serde::de::DeserializeOwned>(
    node: &Node,
    envelope: &SignedPayload,
) -> Result<T> {
    crypto::verify(&node.pubkey, &envelope.payload, &envelope.signature)?;
    let value: Value = serde_json::from_str(&envelope.payload)?;
    let at = value["at"]
        .as_i64()
        .context("signed payload timestamp required")?;
    ensure!(now().abs_diff(at) <= 60, "signed node report expired");
    ensure!(
        value["node_id"] == node.id,
        "signed report belongs to a different node"
    );
    Ok(serde_json::from_value(value)?)
}
pub fn validate_node(pubkey: &str, address: &str) -> Result<()> {
    ensure!(
        crypto::bytes(pubkey)?.len() == 33,
        "compressed Fiber pubkey required"
    );
    secp256k1::PublicKey::from_slice(&crypto::bytes(pubkey)?)?;
    ensure!(
        address.len() < 512
            && !address.contains(char::is_whitespace)
            && address.contains("/tcp/")
            && address.contains("/p2p/"),
        "complete Fiber TCP multiaddr required"
    );
    ensure!(
        address.starts_with("/ip4/")
            || address.starts_with("/ip6/")
            || address.starts_with("/dns4/")
            || address.starts_with("/dns6/"),
        "unsupported Fiber multiaddr"
    );
    Ok(())
}

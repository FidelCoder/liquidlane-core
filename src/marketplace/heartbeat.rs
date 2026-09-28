use super::{
    Market, auth, crypto, db,
    error::{Error, ensure},
    model::{Node, NodeFunding, ProviderPolicy, TESTNET_GENESIS},
    nodes::{SignedPayload, check_payload},
};
use axum::{Json, extract::State, http::HeaderMap};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
struct Heartbeat {
    at: i64,
    available_ckb: u64,
    reserve_ckb: u64,
    version: String,
    chain_hash: String,
    #[serde(default)]
    provider_policy: Option<ProviderPolicy>,
    #[serde(default)]
    funding_address: Option<String>,
    #[serde(default)]
    background: bool,
}

pub async fn heartbeat(
    State(state): State<Market>,
    headers: HeaderMap,
    Json(input): Json<SignedPayload>,
) -> Result<Json<Value>, Error> {
    let node_id = auth::node(&state, &headers)?;
    let existing: Node = state.db.get("node", &node_id)?;
    let report: Heartbeat = check_payload(&existing, &input)?;
    ensure!(
        report.version == state.config.fiber_version && report.chain_hash == TESTNET_GENESIS,
        "node version or network changed"
    );
    ensure!(
        (99..=1000).contains(&report.reserve_ckb),
        "unexpected channel reserve"
    );
    if let Some(policy) = &report.provider_policy {
        ensure!(
            existing.role == "provider"
                && policy.max_order_ckb > 0
                && policy.max_order_ckb <= policy.max_total_ckb,
            "invalid provider funding policy"
        );
    }
    if let Some(address) = &report.funding_address {
        crypto::address_script(address)?;
    }
    state.db.transaction(|tx| {
        let mut node: Node = db::get(tx, "node", &node_id)?;
        ensure!(report.at >= node.last_seen, "stale heartbeat");
        node.last_seen = report.at;
        node.available_ckb = report.available_ckb;
        node.reserve_ckb = report.reserve_ckb;
        node.provider_policy = report.provider_policy;
        node.background = report.background;
        node.funding = report.funding_address.map(|address| NodeFunding {
            address,
            payload: input.payload,
            signature: input.signature,
        });
        db::put(tx, "node", &node_id, &node.owner, &node)
    })?;
    Ok(Json(json!({"accepted":true})))
}

//! Read-only proof of native contract settlement. Wallet arrivals are terminal:
//! their later spending must not be confused with an unresolved channel.
use super::{FIBER_FUNDING_CODE_HASH, chain, crypto, model::now};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

pub const COMMITMENT_CODE_HASH: &str =
    "0x740dee83f87c6f309824d8fd3fbdd3c8380ee6fc9acc90b1a748438afcdf81d8";
const WALLET_CODE_HASH: &str = "0x9bd7e06f3ecf4be0f2fcd2188b23f1b9fcc88e5d4b65a8637b17723bbda3cce8";
pub const MAX_TRANSACTIONS: usize = 32;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Settlement {
    pub closing_tx_hash: String,
    pub transaction_hashes: Vec<String>,
    pub pending_outpoints: Vec<String>,
    pub confirmed: bool,
    pub checked_at: i64,
}

pub(super) async fn transaction(client: &reqwest::Client, url: &str, hash: &str) -> Result<Value> {
    ensure!(
        crypto::bytes(hash)?.len() == 32,
        "invalid settlement transaction hash"
    );
    let response = chain::rpc(client, url, "get_transaction", json!([hash])).await?;
    ensure!(
        response["tx_status"]["status"] == "committed",
        "settlement transaction is not committed"
    );
    let tx = response["transaction"].clone();
    ensure!(
        tx["hash"] == hash,
        "settlement transaction identity mismatch"
    );
    ensure!(
        tx["outputs"].is_array() && tx["inputs"].is_array(),
        "settlement transaction body unavailable"
    );
    Ok(tx)
}

pub(super) fn output<'a>(tx: &'a Value, index: usize) -> Result<&'a Value> {
    tx["outputs"]
        .get(index)
        .context("settlement output missing")
}
pub(super) fn contract(output: &Value, code: &str) -> bool {
    output["type"].is_null()
        && output["lock"]["hash_type"] == "type"
        && output["lock"]["code_hash"] == code
}
fn wallet(tx: &Value, index: usize) -> bool {
    let output = &tx["outputs"][index];
    contract(output, WALLET_CODE_HASH)
        && tx["outputs_data"][index] == "0x"
        && output["lock"]["args"]
            .as_str()
            .and_then(|s| crypto::bytes(s).ok())
            .is_some_and(|s| s.len() == 20)
}
pub(super) fn input_outpoints(tx: &Value) -> Result<Vec<String>> {
    tx["inputs"]
        .as_array()
        .context("settlement inputs missing")?
        .iter()
        .map(|input| {
            let prev = &input["previous_output"];
            chain::canonical_outpoint(&format!(
                "{}#{}",
                prev["tx_hash"]
                    .as_str()
                    .context("settlement input hash missing")?,
                chain::number(&prev["index"])?
            ))
        })
        .collect()
}

/// Reconstruct the graph from committed transactions fetched by our own RPC.
/// Unrecognized outputs, missing spends and incomplete proofs remain pending.
pub(super) fn summarize(
    funding: &str,
    root: &Value,
    transactions: &BTreeMap<String, Value>,
) -> Result<Option<Settlement>> {
    let funding = chain::canonical_outpoint(funding)?;
    let (_, index) = chain::parse_outpoint(&funding)?;
    let cell = output(root, index)?;
    ensure!(
        contract(cell, FIBER_FUNDING_CODE_HASH) && root["outputs_data"][index] == "0x",
        "unexpected native funding output"
    );
    ensure!(
        cell["lock"]["args"]
            .as_str()
            .and_then(|s| crypto::bytes(s).ok())
            .is_some_and(|s| s.len() == 20),
        "unexpected funding lock arguments"
    );
    ensure!(
        transactions.len() <= MAX_TRANSACTIONS,
        "settlement proof too large"
    );
    let mut spends = BTreeMap::new();
    for (hash, tx) in transactions {
        for outpoint in input_outpoints(tx)? {
            ensure!(
                spends.insert(outpoint, hash).is_none(),
                "conflicting settlement spends"
            );
        }
    }
    let Some(closing) = spends.get(&funding) else {
        ensure!(
            transactions.is_empty(),
            "settlement proof does not spend this channel"
        );
        return Ok(None);
    };
    let mut queue = vec![(*closing).clone()];
    let mut visited = BTreeSet::new();
    let mut pending = BTreeSet::new();
    while let Some(hash) = queue.pop() {
        if !visited.insert(hash.clone()) {
            continue;
        }
        let tx = transactions
            .get(&hash)
            .context("settlement transaction missing")?;
        for (index, cell) in tx["outputs"]
            .as_array()
            .context("settlement outputs missing")?
            .iter()
            .enumerate()
        {
            if wallet(tx, index) {
                continue;
            }
            let point = format!("{hash}#{index}");
            if contract(cell, COMMITMENT_CODE_HASH) && tx["outputs_data"][index] == "0x" {
                if let Some(next) = spends.get(&point) {
                    queue.push((*next).clone());
                    continue;
                }
            }
            pending.insert(point);
        }
    }
    ensure!(
        visited.len() == transactions.len(),
        "unrelated settlement transactions"
    );
    Ok(Some(Settlement {
        closing_tx_hash: (*closing).clone(),
        transaction_hashes: visited.into_iter().collect(),
        confirmed: pending.is_empty(),
        pending_outpoints: pending.into_iter().collect(),
        checked_at: now(),
    }))
}

/// Independently check a connector's proof; a signature alone cannot establish recovery.
pub async fn verify(
    client: &reqwest::Client,
    url: &str,
    funding: &str,
    claim: &Settlement,
) -> Result<Settlement> {
    ensure!(
        !claim.transaction_hashes.is_empty() && claim.transaction_hashes.len() <= MAX_TRANSACTIONS,
        "invalid settlement proof size"
    );
    let (hash, _) = chain::parse_outpoint(funding)?;
    let root = transaction(client, url, &hash).await?;
    let mut transactions = BTreeMap::new();
    for hash in &claim.transaction_hashes {
        ensure!(
            !transactions.contains_key(hash),
            "duplicate settlement transaction"
        );
        transactions.insert(hash.clone(), transaction(client, url, hash).await?);
    }
    let verified =
        summarize(funding, &root, &transactions)?.context("channel funding not spent")?;
    ensure!(
        claim.closing_tx_hash == verified.closing_tx_hash
            && claim.confirmed == verified.confirmed
            && claim.pending_outpoints == verified.pending_outpoints,
        "settlement claim differs from committed transactions"
    );
    Ok(verified)
}

#[path = "settlement_scan.rs"]
mod scan;
pub use scan::inspect;

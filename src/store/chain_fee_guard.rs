use anyhow::{Result, anyhow};
use serde_json::Value;

use super::chain_types::{array, hex_index, output_at, outputs, string_field};

const SHANNONS_PER_CKB: u128 = 100_000_000;
const MAX_SETTLEMENT_FEE: u128 = 5 * SHANNONS_PER_CKB;

pub(super) async fn require_reasonable_fee(
    client: &crate::ckb_rpc::CkbRpcClient,
    transaction: &Value,
    label: &str,
) -> Result<()> {
    let input_capacity = input_capacity(client, transaction).await?;
    let output_capacity = outputs(transaction)?
        .into_iter()
        .map(|output| output.capacity)
        .sum::<u128>();
    let fee = input_capacity
        .checked_sub(output_capacity)
        .ok_or_else(|| anyhow!("{label} transaction output capacity exceeds inputs"))?;
    if fee <= MAX_SETTLEMENT_FEE {
        return Ok(());
    }
    Err(anyhow!(
        "{label} transaction fee is too high: {} CKB; receipt capacity must return to the wallet",
        format_ckb(fee)
    ))
}

async fn input_capacity(
    client: &crate::ckb_rpc::CkbRpcClient,
    transaction: &Value,
) -> Result<u128> {
    let mut total = 0u128;
    for input in array(transaction, "inputs")? {
        let previous = input
            .get("previous_output")
            .ok_or_else(|| anyhow!("transaction input previous_output is missing"))?;
        let previous_tx = client
            .transaction_details(string_field(previous, "tx_hash")?)
            .await?
            .transaction;
        let output = output_at(&previous_tx, hex_index(string_field(previous, "index")?)?)?;
        total = total
            .checked_add(output.capacity)
            .ok_or_else(|| anyhow!("transaction input capacity overflow"))?;
    }
    Ok(total)
}

fn format_ckb(shannons: u128) -> String {
    let whole = shannons / SHANNONS_PER_CKB;
    let fraction = shannons % SHANNONS_PER_CKB;
    if fraction == 0 {
        return whole.to_string();
    }
    let mut tail = format!("{fraction:08}");
    while tail.ends_with('0') {
        tail.pop();
    }
    format!("{whole}.{tail}")
}

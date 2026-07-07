use anyhow::{Result, anyhow};
use serde_json::Value;

use super::{
    AppStore,
    chain_types::{
        ChainOutput, ChainScript, array, hex_index, output_at, outputs, parse_receipt_data,
        parse_vault_data, required_hash, script_from_address, string_field, type_code_matches,
    },
};
use crate::domain::{LpPosition, User, WithdrawalIntent};

pub(super) const SHANNONS_PER_CKB: u128 = 100_000_000;
pub(super) const ARG_SEGMENT_CHARS: usize = 64;

impl AppStore {
    pub(super) async fn verify_withdrawal_tx(
        &self,
        tx_hash: &str,
        intent: &WithdrawalIntent,
        position: &LpPosition,
        user: &User,
        signed_tx: &Option<Value>,
    ) -> Result<()> {
        self.verify_ckb_settlement_tx(tx_hash, signed_tx).await?;
        let Some(client) = self.ckb_rpc.as_ref() else {
            return Ok(());
        };
        let transaction = client.transaction_details(tx_hash).await?.transaction;
        let user_lock = script_from_address(&user.ckb_address)?;
        let vault_lock = vault_lock_script(&self.vault)?;
        let vault_type_code = required_hash(
            self.vault.scripts.vault_type_code_hash.as_deref(),
            "LIQUIDLANE_VAULT_TYPE_CODE_HASH",
        )?;
        let receipt_type_code = required_hash(
            self.vault.scripts.lp_receipt_type_code_hash.as_deref(),
            "LIQUIDLANE_LP_RECEIPT_TYPE_CODE_HASH",
        )?;
        let previous_vault =
            previous_vault_cell(client, &transaction, &vault_lock, &vault_type_code).await?;
        let next_vault = next_vault_cell(&transaction, &vault_lock, &vault_type_code)?;
        require_vault_withdrawal_delta(&previous_vault, &next_vault, intent.amount)?;
        let previous_receipt = previous_receipt_cell(
            client,
            &transaction,
            position,
            &user_lock,
            &receipt_type_code,
        )
        .await?;
        require_withdrawal_receipt_delta(&transaction, &previous_receipt, intent, position)
    }
}

pub(super) async fn previous_vault_cell(
    client: &crate::ckb_rpc::CkbRpcClient,
    transaction: &Value,
    vault_lock: &ChainScript,
    vault_type_code: &str,
) -> Result<ChainOutput> {
    let mut found = Vec::new();
    for input in array(transaction, "inputs")? {
        let previous = input
            .get("previous_output")
            .ok_or_else(|| anyhow!("transaction input previous_output is missing"))?;
        let previous_tx = client
            .transaction_details(string_field(previous, "tx_hash")?)
            .await?
            .transaction;
        let output = output_at(&previous_tx, hex_index(string_field(previous, "index")?)?)?;
        if output.lock == *vault_lock && type_code_matches(&output.type_script, vault_type_code) {
            found.push(output);
        }
    }
    single(
        found,
        "transaction did not spend the active vault cell",
        "transaction spent duplicate vault cells",
    )
}

pub(super) async fn previous_receipt_cell(
    client: &crate::ckb_rpc::CkbRpcClient,
    transaction: &Value,
    position: &LpPosition,
    user_lock: &ChainScript,
    receipt_type_code: &str,
) -> Result<ChainOutput> {
    let out_point = receipt_out_point(position);
    for input in array(transaction, "inputs")? {
        let previous = input
            .get("previous_output")
            .ok_or_else(|| anyhow!("transaction input previous_output is missing"))?;
        if !out_point_matches(previous, &out_point)? {
            continue;
        }
        let previous_tx = client
            .transaction_details(string_field(previous, "tx_hash")?)
            .await?
            .transaction;
        let output = output_at(&previous_tx, hex_index(string_field(previous, "index")?)?)?;
        if output.lock == *user_lock && type_code_matches(&output.type_script, receipt_type_code) {
            return Ok(output);
        }
    }
    Err(anyhow!("transaction did not spend the LP receipt cell"))
}

pub(super) fn next_vault_cell(
    transaction: &Value,
    vault_lock: &ChainScript,
    vault_type_code: &str,
) -> Result<ChainOutput> {
    single(
        outputs(transaction)?
            .into_iter()
            .filter(|output| {
                output.lock == *vault_lock
                    && type_code_matches(&output.type_script, vault_type_code)
            })
            .collect(),
        "transaction did not recreate the active vault cell",
        "transaction created duplicate vault cells",
    )
}

pub(super) fn next_receipt_cell(
    transaction: &Value,
    previous: &ChainOutput,
) -> Result<ChainOutput> {
    let previous_type = previous
        .type_script
        .as_ref()
        .ok_or_else(|| anyhow!("LP receipt type script is missing"))?;
    single(
        outputs(transaction)?
            .into_iter()
            .filter(|output| {
                output.lock == previous.lock && output.type_script.as_ref() == Some(previous_type)
            })
            .collect(),
        "transaction did not recreate the LP receipt cell",
        "transaction created duplicate LP receipt cells",
    )
}

pub(super) fn vault_lock_script(vault: &crate::domain::VaultConfig) -> Result<ChainScript> {
    let address = vault
        .address
        .as_deref()
        .ok_or_else(|| anyhow!("LIQUIDLANE_VAULT_CKB_ADDRESS is missing"))?;
    script_from_address(address)
}

pub(super) fn padded_id(id: &uuid::Uuid) -> String {
    let mut raw = id
        .to_string()
        .chars()
        .filter(|ch| ch.is_ascii_hexdigit())
        .take(ARG_SEGMENT_CHARS)
        .collect::<String>();
    while raw.len() < ARG_SEGMENT_CHARS {
        raw.push('0');
    }
    format!("0x{raw}")
}

pub(super) fn join_hex(values: &[String]) -> String {
    let mut out = String::from("0x");
    for value in values {
        out.push_str(value.trim_start_matches("0x"));
    }
    out
}

pub(super) fn single(
    mut values: Vec<ChainOutput>,
    missing: &str,
    duplicate: &str,
) -> Result<ChainOutput> {
    match values.len() {
        1 => Ok(values.remove(0)),
        0 => Err(anyhow!(missing.to_string())),
        _ => Err(anyhow!(duplicate.to_string())),
    }
}

pub(super) fn require_vault_fee_delta(
    previous: &ChainOutput,
    next: &ChainOutput,
    amount: u64,
) -> Result<()> {
    let previous_data = parse_vault_data(&previous.data)?;
    let next_data = parse_vault_data(&next.data)?;
    let capacity_delta = u128::from(amount) * SHANNONS_PER_CKB;
    if next_data.total == previous_data.total
        && next_data.reserved == previous_data.reserved
        && next_data.deployed == previous_data.deployed
        && next_data.fee_balance == previous_data.fee_balance.saturating_sub(amount)
        && previous.capacity >= next.capacity.saturating_add(capacity_delta)
    {
        return Ok(());
    }
    Err(anyhow!("fee claim transaction vault delta is invalid"))
}

fn require_vault_withdrawal_delta(
    previous: &ChainOutput,
    next: &ChainOutput,
    amount: u64,
) -> Result<()> {
    let previous_data = parse_vault_data(&previous.data)?;
    let next_data = parse_vault_data(&next.data)?;
    let capacity_delta = u128::from(amount) * SHANNONS_PER_CKB;
    if next_data.total == previous_data.total.saturating_sub(amount)
        && next_data.reserved == previous_data.reserved
        && next_data.deployed == previous_data.deployed
        && next_data.fee_balance == previous_data.fee_balance
        && previous.capacity >= next.capacity.saturating_add(capacity_delta)
    {
        return Ok(());
    }
    Err(anyhow!("withdrawal transaction vault delta is invalid"))
}

fn require_withdrawal_receipt_delta(
    transaction: &Value,
    previous: &ChainOutput,
    intent: &WithdrawalIntent,
    position: &LpPosition,
) -> Result<()> {
    let before = parse_receipt_data(&previous.data)?;
    if intent.amount == position.supplied_amount {
        return Ok(());
    }
    let next = next_receipt_cell(transaction, previous)?;
    let after = parse_receipt_data(&next.data)?;
    if after.supplied == before.supplied.saturating_sub(intent.amount)
        && after.available == before.available.saturating_sub(intent.amount)
        && after.reserved == before.reserved
        && after.deployed == before.deployed
        && after.claimed == before.claimed
    {
        return Ok(());
    }
    Err(anyhow!(
        "withdrawal transaction LP receipt delta is invalid"
    ))
}

fn out_point_matches(value: &Value, expected: &str) -> Result<bool> {
    let Some((hash, index)) = expected.split_once('#') else {
        return Err(anyhow!("LP receipt out-point is invalid"));
    };
    Ok(string_field(value, "tx_hash")?.eq_ignore_ascii_case(hash)
        && string_field(value, "index")?.eq_ignore_ascii_case(index))
}

fn receipt_out_point(position: &LpPosition) -> String {
    position
        .receipt_cell_out_point
        .clone()
        .unwrap_or_else(|| format!("{}#0x1", position.supply_tx_hash))
}

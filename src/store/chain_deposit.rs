use anyhow::{Result, anyhow};
use serde_json::Value;

use super::{
    AppStore,
    chain_types::{
        ChainOutput, ChainScript, array, hex_index, output_at, outputs, parse_receipt_data,
        parse_vault_data, required_hash, script_from_address, script_hash, string_field,
        type_code_matches,
    },
};
use crate::domain::{SupplyIntent, User};

const SHANNONS_PER_CKB: u128 = 100_000_000;

impl AppStore {
    pub(super) async fn verify_vault_deposit_tx(
        &self,
        tx_hash: &str,
        intent: &SupplyIntent,
        user: &User,
        signed_tx: &Option<Value>,
    ) -> Result<()> {
        self.verify_ckb_settlement_tx(tx_hash, signed_tx).await?;
        let Some(client) = self.ckb_rpc.as_ref() else {
            return Ok(());
        };

        let current = client.transaction_details(tx_hash).await?.transaction;
        let vault = self.vault_config().await;
        let vault_lock = vault_lock_script(&vault)?;
        let user_lock = script_from_address(&user.ckb_address)?;
        let vault_type_code = required_hash(
            vault.scripts.vault_type_code_hash.as_deref(),
            "LIQUIDLANE_VAULT_TYPE_CODE_HASH",
        )?;
        let receipt_type_code = required_hash(
            vault.scripts.lp_receipt_type_code_hash.as_deref(),
            "LIQUIDLANE_LP_RECEIPT_TYPE_CODE_HASH",
        )?;
        let request_type_code = required_hash(
            vault.scripts.request_type_code_hash.as_deref(),
            "LIQUIDLANE_REQUEST_TYPE_CODE_HASH",
        )?;
        let fee_claim_type_code = required_hash(
            vault.scripts.fee_claim_type_code_hash.as_deref(),
            "LIQUIDLANE_FEE_CLAIM_TYPE_CODE_HASH",
        )?;

        let (previous_vault, previous_type) =
            previous_vault_cell(client, &current, &vault_lock, &vault_type_code).await?;
        let next_vault = next_vault_cell(&current, &vault_lock, &previous_type)?;
        let receipt = receipt_cell(&current, &user_lock, &receipt_type_code)?;

        require_vault_delta(previous_vault, next_vault, intent.amount)?;
        require_receipt_identity(
            &receipt,
            &previous_type,
            &user_lock,
            &request_type_code,
            &fee_claim_type_code,
            intent,
        )?;
        require_receipt(receipt, intent.amount)
    }
}

async fn previous_vault_cell(
    client: &crate::ckb_rpc::CkbRpcClient,
    transaction: &Value,
    vault_lock: &ChainScript,
    vault_type_code: &str,
) -> Result<(ChainOutput, ChainScript)> {
    let mut matches = Vec::new();
    for input in array(transaction, "inputs")? {
        let previous = input
            .get("previous_output")
            .ok_or_else(|| anyhow!("transaction input previous_output is missing"))?;
        let tx_hash = string_field(previous, "tx_hash")?;
        let index = hex_index(string_field(previous, "index")?)?;
        let previous_tx = client.transaction_details(tx_hash).await?.transaction;
        let output = output_at(&previous_tx, index)?;
        if output.lock == *vault_lock && type_code_matches(&output.type_script, vault_type_code) {
            let type_script = output
                .type_script
                .clone()
                .ok_or_else(|| anyhow!("vault input type script is missing"))?;
            matches.push((output, type_script));
        }
    }
    match matches.len() {
        1 => Ok(matches.remove(0)),
        0 => Err(anyhow!(
            "supply transaction did not spend the active vault cell"
        )),
        _ => Err(anyhow!("supply transaction spent more than one vault cell")),
    }
}

fn next_vault_cell(
    transaction: &Value,
    vault_lock: &ChainScript,
    vault_type: &ChainScript,
) -> Result<ChainOutput> {
    let matches = outputs(transaction)?
        .into_iter()
        .filter(|output| {
            output.lock == *vault_lock && output.type_script.as_ref() == Some(vault_type)
        })
        .collect::<Vec<_>>();
    match matches.len() {
        1 => Ok(matches.into_iter().next().unwrap()),
        0 => Err(anyhow!(
            "supply transaction did not recreate the vault cell"
        )),
        _ => Err(anyhow!("supply transaction created duplicate vault cells")),
    }
}

fn receipt_cell(
    transaction: &Value,
    user_lock: &ChainScript,
    receipt_type_code: &str,
) -> Result<ChainOutput> {
    let matches = outputs(transaction)?
        .into_iter()
        .filter(|output| {
            output.lock == *user_lock && type_code_matches(&output.type_script, receipt_type_code)
        })
        .collect::<Vec<_>>();
    match matches.len() {
        1 => Ok(matches.into_iter().next().unwrap()),
        0 => Err(anyhow!(
            "supply transaction did not mint an LP receipt cell"
        )),
        _ => Err(anyhow!(
            "supply transaction minted duplicate LP receipt cells"
        )),
    }
}

fn require_vault_delta(previous: ChainOutput, next: ChainOutput, amount: u64) -> Result<()> {
    let previous_data = parse_vault_data(&previous.data)?;
    let next_data = parse_vault_data(&next.data)?;
    if next_data.total != previous_data.total.saturating_add(amount)
        || next_data.reserved != previous_data.reserved
        || next_data.deployed != previous_data.deployed
        || next_data.fee_balance != previous_data.fee_balance
    {
        return Err(anyhow!(
            "supply transaction vault accounting delta is invalid"
        ));
    }
    let required_capacity = previous
        .capacity
        .checked_add(u128::from(amount) * SHANNONS_PER_CKB)
        .ok_or_else(|| anyhow!("vault capacity delta overflow"))?;
    if next.capacity < required_capacity {
        return Err(anyhow!(
            "supply transaction did not add enough CKB capacity to the vault"
        ));
    }
    Ok(())
}

fn require_receipt_identity(
    output: &ChainOutput,
    vault_type: &ChainScript,
    user_lock: &ChainScript,
    request_type_code: &str,
    fee_claim_type_code: &str,
    intent: &SupplyIntent,
) -> Result<()> {
    let Some(receipt_type) = output.type_script.as_ref() else {
        return Err(anyhow!(
            "supply transaction LP receipt type script is missing"
        ));
    };
    let expected_args = join_hex(&[
        script_hash(vault_type)?,
        script_hash(user_lock)?,
        request_type_code.to_string(),
        fee_claim_type_code.to_string(),
        asset_id(&intent.asset),
        intent_id(&intent.id),
    ]);
    if receipt_type.hash_type != "data1" || receipt_type.args != expected_args {
        return Err(anyhow!(
            "supply transaction LP receipt type args do not match the supply intent"
        ));
    }
    Ok(())
}

fn require_receipt(output: ChainOutput, amount: u64) -> Result<()> {
    let receipt = parse_receipt_data(&output.data)?;
    if receipt.supplied == amount
        && receipt.available == amount
        && receipt.reserved == 0
        && receipt.deployed == 0
        && receipt.claimed == 0
    {
        return Ok(());
    }
    Err(anyhow!("supply transaction LP receipt data is invalid"))
}

fn join_hex(values: &[String]) -> String {
    let mut out = String::from("0x");
    for value in values {
        out.push_str(value.trim_start_matches("0x"));
    }
    out
}

fn asset_id(asset: &str) -> String {
    let mut bytes = [0u8; 32];
    for (index, byte) in asset
        .trim()
        .to_ascii_uppercase()
        .bytes()
        .take(32)
        .enumerate()
    {
        bytes[index] = byte;
    }
    hex(&bytes)
}

fn intent_id(id: &uuid::Uuid) -> String {
    let mut raw = id
        .to_string()
        .chars()
        .filter(|ch| ch.is_ascii_hexdigit())
        .take(64)
        .collect::<String>();
    while raw.len() < 64 {
        raw.push('0');
    }
    format!("0x{raw}")
}

fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(2 + bytes.len() * 2);
    out.push_str("0x");
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

fn vault_lock_script(vault: &crate::domain::VaultConfig) -> Result<ChainScript> {
    let address = vault
        .address
        .as_deref()
        .ok_or_else(|| anyhow!("LIQUIDLANE_VAULT_CKB_ADDRESS is missing"))?;
    script_from_address(address)
}

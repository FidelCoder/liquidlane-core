use anyhow::{Result, anyhow};
use serde_json::Value;

use super::{
    AppStore,
    chain_types::{
        ChainOutput, ChainScript, array, hex_index, output_at, outputs, parse_request_data,
        parse_vault_data, required_hash, script_from_address, script_hash, string_field,
        type_code_matches,
    },
};
use crate::domain::LiquidityRequest;

const ARG_SEGMENT_CHARS: usize = 64;
const REQUEST_ARGS_LEN: usize = 2 + ARG_SEGMENT_CHARS * 4;

impl AppStore {
    pub(super) async fn verify_capacity_request_tx(
        &self,
        request: &LiquidityRequest,
        signed_tx: &Option<Value>,
    ) -> Result<()> {
        let Some(tx_hash) = request.request_tx_hash.as_deref() else {
            return Ok(());
        };
        self.verify_ckb_settlement_tx(tx_hash, signed_tx).await?;
        let Some(client) = self.ckb_rpc.as_ref() else {
            return Ok(());
        };

        let transaction = client.transaction_details(tx_hash).await?.transaction;
        let vault = self.vault_config().await;
        let merchant_lock = script_from_address(&request.ckb_address)?;
        let vault_lock = vault_lock_script(&vault)?;
        let vault_type_code = required_hash(
            vault.scripts.vault_type_code_hash.as_deref(),
            "LIQUIDLANE_VAULT_TYPE_CODE_HASH",
        )?;
        let request_type_code = required_hash(
            vault.scripts.request_type_code_hash.as_deref(),
            "LIQUIDLANE_REQUEST_TYPE_CODE_HASH",
        )?;

        let previous_vault =
            previous_vault_cell(client, &transaction, &vault_lock, &vault_type_code).await?;
        let next_vault = next_vault_cell(&transaction, &vault_lock, &vault_type_code)?;
        require_vault_reservation_delta(
            &previous_vault,
            &next_vault,
            request.amount,
            request.lease_fee,
        )?;

        let vault_type = next_vault
            .type_script
            .as_ref()
            .ok_or_else(|| anyhow!("request transaction vault output type script is missing"))?;
        let (request_index, output) =
            request_cell(&transaction, &merchant_lock, &request_type_code)?;
        require_request_identity(&output, vault_type, &merchant_lock, request)?;
        require_request_data(&output, request)?;
        require_declared_out_point(request, tx_hash, request_index)?;
        Ok(())
    }
}

async fn previous_vault_cell(
    client: &crate::ckb_rpc::CkbRpcClient,
    transaction: &Value,
    vault_lock: &ChainScript,
    vault_type_code: &str,
) -> Result<ChainOutput> {
    let mut matches = Vec::new();
    for input in array(transaction, "inputs")? {
        let previous = input
            .get("previous_output")
            .ok_or_else(|| anyhow!("request transaction input previous_output is missing"))?;
        let tx_hash = string_field(previous, "tx_hash")?;
        let index = hex_index(string_field(previous, "index")?)?;
        let previous_tx = client.transaction_details(tx_hash).await?.transaction;
        let output = output_at(&previous_tx, index)?;
        if output.lock == *vault_lock && type_code_matches(&output.type_script, vault_type_code) {
            matches.push(output);
        }
    }
    match matches.len() {
        1 => Ok(matches.remove(0)),
        0 => Err(anyhow!(
            "request transaction did not spend the active vault cell"
        )),
        _ => Err(anyhow!(
            "request transaction spent more than one vault cell"
        )),
    }
}

fn next_vault_cell(
    transaction: &Value,
    vault_lock: &ChainScript,
    vault_type_code: &str,
) -> Result<ChainOutput> {
    let matches = outputs(transaction)?
        .into_iter()
        .filter(|output| {
            output.lock == *vault_lock && type_code_matches(&output.type_script, vault_type_code)
        })
        .collect::<Vec<_>>();
    match matches.len() {
        1 => Ok(matches.into_iter().next().unwrap()),
        0 => Err(anyhow!(
            "request transaction did not recreate the active vault cell"
        )),
        _ => Err(anyhow!("request transaction created duplicate vault cells")),
    }
}

fn request_cell(
    transaction: &Value,
    merchant_lock: &ChainScript,
    request_type_code: &str,
) -> Result<(usize, ChainOutput)> {
    let matches = outputs(transaction)?
        .into_iter()
        .enumerate()
        .filter(|(_, output)| {
            output.lock == *merchant_lock
                && type_code_matches(&output.type_script, request_type_code)
        })
        .collect::<Vec<_>>();
    match matches.len() {
        1 => Ok(matches.into_iter().next().unwrap()),
        0 => Err(anyhow!(
            "request transaction did not create the merchant capacity request cell"
        )),
        _ => Err(anyhow!(
            "request transaction created duplicate merchant request cells"
        )),
    }
}

fn require_vault_reservation_delta(
    previous: &ChainOutput,
    next: &ChainOutput,
    amount: u64,
    lease_fee: u64,
) -> Result<()> {
    let previous_data = parse_vault_data(&previous.data)?;
    let next_data = parse_vault_data(&next.data)?;
    let required_capacity = previous
        .capacity
        .checked_add(u128::from(lease_fee) * 100_000_000)
        .ok_or_else(|| anyhow!("request vault capacity delta overflow"))?;
    if next_data.total == previous_data.total
        && next_data.reserved == previous_data.reserved.saturating_add(amount)
        && next_data.deployed == previous_data.deployed
        && next_data.fee_balance == previous_data.fee_balance.saturating_add(lease_fee)
        && next.capacity >= required_capacity
    {
        return Ok(());
    }
    Err(anyhow!(
        "request transaction vault reservation delta is invalid"
    ))
}

fn require_request_identity(
    output: &ChainOutput,
    vault_type: &ChainScript,
    merchant_lock: &ChainScript,
    request: &LiquidityRequest,
) -> Result<()> {
    let Some(type_script) = output.type_script.as_ref() else {
        return Err(anyhow!("request cell type script is missing"));
    };
    if type_script.hash_type != "data1" || type_script.args.len() != REQUEST_ARGS_LEN {
        return Err(anyhow!("request cell type args are invalid"));
    }
    require_arg_segment(
        &type_script.args,
        0,
        &script_hash(vault_type)?,
        "vault type",
    )?;
    require_arg_segment(
        &type_script.args,
        1,
        &script_hash(merchant_lock)?,
        "merchant lock",
    )?;
    require_arg_segment(&type_script.args, 3, &request_id(&request.id), "request id")?;
    if arg_segment(&type_script.args, 2)?
        .chars()
        .all(|ch| ch == '0')
    {
        return Err(anyhow!("request cell operator lock hash is missing"));
    }
    Ok(())
}

fn require_request_data(output: &ChainOutput, request: &LiquidityRequest) -> Result<()> {
    let data = parse_request_data(&output.data)?;
    if (data.status == 0 || data.status == 1)
        && data.amount == request.amount
        && data.lease_fee == request.lease_fee
    {
        return Ok(());
    }
    Err(anyhow!(
        "request cell data does not match the request intent"
    ))
}

fn require_declared_out_point(
    request: &LiquidityRequest,
    tx_hash: &str,
    output_index: usize,
) -> Result<()> {
    let Some(out_point) = request.request_cell_out_point.as_deref() else {
        return Ok(());
    };
    let (declared_hash, declared_index) = parse_out_point(out_point)?;
    if declared_hash.eq_ignore_ascii_case(tx_hash) && declared_index == output_index {
        return Ok(());
    }
    Err(anyhow!(
        "request_cell_out_point does not point to the verified request cell"
    ))
}

fn require_arg_segment(args: &str, index: usize, expected: &str, label: &str) -> Result<()> {
    if arg_segment(args, index)?.eq_ignore_ascii_case(expected.trim_start_matches("0x")) {
        Ok(())
    } else {
        Err(anyhow!("request cell {label} arg does not match"))
    }
}

fn arg_segment(args: &str, index: usize) -> Result<&str> {
    let raw = args
        .strip_prefix("0x")
        .ok_or_else(|| anyhow!("request cell args must be 0x-prefixed"))?;
    let start = index * ARG_SEGMENT_CHARS;
    raw.get(start..start + ARG_SEGMENT_CHARS)
        .ok_or_else(|| anyhow!("request cell args segment is missing"))
}

fn parse_out_point(out_point: &str) -> Result<(String, usize)> {
    let (tx_hash, index) = out_point
        .split_once('#')
        .ok_or_else(|| anyhow!("request_cell_out_point must be tx_hash#index"))?;
    Ok((tx_hash.to_string(), hex_index(index)?))
}

fn request_id(id: &uuid::Uuid) -> String {
    let mut raw = id
        .to_string()
        .chars()
        .filter(|ch| ch.is_ascii_hexdigit())
        .take(ARG_SEGMENT_CHARS)
        .collect::<String>();
    while raw.len() < ARG_SEGMENT_CHARS {
        raw.push('0');
    }
    raw
}

fn vault_lock_script(vault: &crate::domain::VaultConfig) -> Result<ChainScript> {
    let address = vault
        .address
        .as_deref()
        .ok_or_else(|| anyhow!("LIQUIDLANE_VAULT_CKB_ADDRESS is missing"))?;
    script_from_address(address)
}

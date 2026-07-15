use std::str::FromStr;

use anyhow::{Context, Result, anyhow};
use ckb_jsonrpc_types::Transaction as JsonTransaction;
use ckb_sdk::{
    CkbRpcClient, NetworkInfo, NetworkType, ScriptGroup, TransactionWithScriptGroups,
    transaction::signer::{SignContexts, TransactionSigner},
};
use ckb_types::{H256, core::TransactionView, packed, prelude::*};

use super::{
    fiber_funding_builder::{
        BuiltFiberFundingTx, FiberFundingBuilderPayload, MatchedFundingRequest,
    },
    fiber_funding_cells::{
        add_default_lock_dep, add_script_deps, funding_cell, live_cell, next_request_cell,
        next_vault_cell,
    },
    fiber_funding_hex::{
        packed_script_entity_hex, parse_out_point, script_from_address, secp_placeholder_witness,
    },
};

const SHANNONS_PER_CKB: u64 = 100_000_000;

pub(super) fn build_vault_funding_transaction(
    matched: MatchedFundingRequest,
    payload: FiberFundingBuilderPayload,
    signer_private_key: Option<String>,
) -> Result<BuiltFiberFundingTx> {
    let rpc = CkbRpcClient::new(&payload.rpc_url);
    let vault_out_point = parse_out_point(
        matched
            .vault
            .cell_out_point
            .as_deref()
            .ok_or_else(|| anyhow!("LIQUIDLANE_VAULT_CELL_OUT_POINT is missing"))?,
    )?;
    let request_out_point = parse_out_point(
        matched
            .request
            .request_cell_out_point
            .as_deref()
            .ok_or_else(|| anyhow!("request cell out-point is missing"))?,
    )?;
    let vault_cell = live_cell(&rpc, vault_out_point)?;
    let request_cell = live_cell(&rpc, request_out_point)?;
    let executor_address = matched
        .vault
        .executor_address
        .as_deref()
        .ok_or_else(|| anyhow!("LIQUIDLANE_EXECUTOR_CKB_ADDRESS is missing"))?;
    let executor_lock = script_from_address(executor_address)?;
    let funding_source_lock = packed_script_entity_hex(&payload.funding_source_lock_script)?;
    if funding_source_lock.calc_script_hash() != executor_lock.calc_script_hash() {
        return Err(anyhow!(
            "Fiber funding source lock does not match the configured LiquidLane executor"
        ));
    }
    if request_cell.output.lock().calc_script_hash() != executor_lock.calc_script_hash() {
        return Err(anyhow!(
            "LiquidLane request cell lock does not match the configured executor signer"
        ));
    }
    let funding_lock = packed_script_entity_hex(&payload.request.script)?;
    let base: TransactionView = Into::<packed::Transaction>::into(payload.tx.clone()).into_view();
    let funded_shannons = payload
        .request
        .local_amount
        .checked_add(u128::from(payload.request.local_reserved_ckb_amount))
        .ok_or_else(|| anyhow!("Fiber funding amount exceeds u128 shannon range"))?;
    let local_shannons = u64::try_from(funded_shannons)
        .map_err(|_| anyhow!("Fiber local amount exceeds u64 shannon range"))?;
    let expected_shannons = matched
        .request
        .amount
        .checked_mul(SHANNONS_PER_CKB)
        .ok_or_else(|| anyhow!("request funding amount overflow"))?;
    if local_shannons != expected_shannons {
        return Err(anyhow!(
            "Fiber funding amount does not match reserved request amount"
        ));
    }

    let base_outputs = base.outputs().into_iter().collect::<Vec<_>>();
    if base_outputs.len() > 1 {
        return Err(anyhow!(
            "LiquidLane vault funding supports Fiber transactions with one funding output"
        ));
    }
    let funding_cell = funding_cell(&payload, funding_lock.clone())?;
    let next_vault = next_vault_cell(&vault_cell, local_shannons)?;
    let next_request = next_request_cell(&request_cell, 0)?;
    let mut inputs = base.inputs().into_iter().collect::<Vec<_>>();
    let base_input_len = inputs.len();
    inputs.push(vault_cell.input.clone());
    inputs.push(request_cell.input.clone());

    let outputs = vec![funding_cell.output, next_vault.output, next_request.output];
    let outputs_data = vec![funding_cell.data, next_vault.data, next_request.data];
    let mut cell_deps = base.cell_deps().into_iter().collect::<Vec<_>>();
    add_script_deps(&mut cell_deps, &matched.vault)?;
    add_default_lock_dep(&mut cell_deps, &rpc, &executor_lock)?;
    let mut witnesses = base.witnesses().into_iter().collect::<Vec<_>>();
    while witnesses.len() < base_input_len {
        witnesses.push(packed::Bytes::default());
    }
    witnesses.push(packed::Bytes::default());
    witnesses.push(secp_placeholder_witness());
    let tx = base
        .as_advanced_builder()
        .set_cell_deps(cell_deps)
        .set_inputs(inputs)
        .set_outputs(outputs)
        .set_outputs_data(outputs_data)
        .set_witnesses(witnesses)
        .build();
    let tx = sign_executor_inputs(
        tx,
        &payload.rpc_url,
        &executor_lock,
        &[base_input_len + 1],
        signer_private_key.as_deref(),
    )?;
    let tx_hash = tx_hash_hex(&tx);
    let transaction: JsonTransaction = tx.data().into();
    Ok(BuiltFiberFundingTx {
        transaction: serde_json::to_value(transaction)?,
        funding_out_point: format!("{}#0x0", tx_hash),
        tx_hash,
        request_id: matched.request.id,
    })
}

fn sign_executor_inputs(
    tx: TransactionView,
    rpc_url: &str,
    executor_lock: &packed::Script,
    input_indices: &[usize],
    private_key: Option<&str>,
) -> Result<TransactionView> {
    let private_key = private_key.ok_or_else(|| {
        anyhow!("LIQUIDLANE_EXECUTOR_PRIVATE_KEY is required to sign vault-funded executor inputs")
    })?;
    let network_info = NetworkInfo::new(NetworkType::Testnet, rpc_url.to_string());
    let mut script_group = ScriptGroup::from_lock_script(executor_lock);
    script_group.input_indices = input_indices.to_vec();
    let mut tx_with_groups = TransactionWithScriptGroups::new(tx, vec![script_group]);
    let signed = TransactionSigner::new(&network_info).sign_transaction(
        &mut tx_with_groups,
        &SignContexts::new_sighash_h256(vec![parse_private_key(private_key)?])?,
    )?;
    if signed.is_empty() {
        return Err(anyhow!(
            "vault funding signer could not sign the executor lock group"
        ));
    }
    Ok(tx_with_groups.get_tx_view().clone())
}

fn parse_private_key(value: &str) -> Result<H256> {
    let value = value.trim().trim_start_matches("0x");
    H256::from_str(value).context("LIQUIDLANE_EXECUTOR_PRIVATE_KEY must be 32-byte hex")
}

fn tx_hash_hex(tx: &TransactionView) -> String {
    let hash: H256 = tx.hash().unpack();
    hash.to_string()
}

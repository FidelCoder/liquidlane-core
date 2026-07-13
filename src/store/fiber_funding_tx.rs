use anyhow::{Result, anyhow};
use ckb_jsonrpc_types::Transaction as JsonTransaction;
use ckb_sdk::CkbRpcClient;
use ckb_types::{core::TransactionView, packed, prelude::*};

use super::{
    fiber_funding_builder::{
        BuiltFiberFundingTx, FiberFundingBuilderPayload, MatchedFundingRequest,
    },
    fiber_funding_cells::{
        add_default_lock_dep, add_script_deps, executor_cell, executor_change_cell, funding_cell,
        funding_intent_cell, live_cell, next_request_cell, next_vault_cell,
    },
    fiber_funding_hex::{packed_script_entity_hex, parse_out_point, secp_placeholder_witness},
};

const SHANNONS_PER_CKB: u64 = 100_000_000;

pub(super) fn build_vault_funding_transaction(
    matched: MatchedFundingRequest,
    payload: FiberFundingBuilderPayload,
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
    let executor = executor_cell(&payload.rpc_url, executor_address)?;
    let executor_lock = executor.output.lock();
    let funding_source_lock = packed_script_entity_hex(&payload.funding_source_lock_script)?;
    if funding_source_lock.calc_script_hash() != executor_lock.calc_script_hash() {
        return Err(anyhow!(
            "Fiber funding source lock does not match the configured LiquidLane executor"
        ));
    }
    let funding_lock = packed_script_entity_hex(&payload.request.script)?;
    let base: TransactionView = Into::<packed::Transaction>::into(payload.tx.clone()).into_view();
    let local_shannons = u64::try_from(payload.request.local_amount)
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

    let request_type = request_cell
        .output
        .type_()
        .to_opt()
        .ok_or_else(|| anyhow!("request cell type script is missing"))?;
    let vault_type = vault_cell
        .output
        .type_()
        .to_opt()
        .ok_or_else(|| anyhow!("vault cell type script is missing"))?;
    let funding_cell = funding_cell(&payload, funding_lock.clone())?;
    let next_vault = next_vault_cell(&vault_cell, local_shannons)?;
    let next_request = next_request_cell(&request_cell)?;
    let funding_intent = funding_intent_cell(
        &matched,
        &vault_type,
        &request_type,
        &executor_lock,
        &funding_lock,
    )?;
    let change = executor_change_cell(&executor, &funding_intent)?;

    let mut inputs = base.inputs().into_iter().collect::<Vec<_>>();
    let base_input_len = inputs.len();
    inputs.push(vault_cell.input.clone());
    inputs.push(request_cell.input.clone());
    inputs.push(executor.input.clone());

    let mut outputs = vec![funding_cell.output];
    let mut outputs_data = vec![funding_cell.data];
    for (index, output) in base.outputs().into_iter().enumerate().skip(1) {
        outputs.push(output);
        outputs_data.push(base.outputs_data().get(index).unwrap_or_default());
    }
    for output in [next_vault, next_request, funding_intent] {
        outputs.push(output.output);
        outputs_data.push(output.data);
    }
    if let Some(change) = change {
        outputs.push(change.output);
        outputs_data.push(change.data);
    }

    let mut cell_deps = base.cell_deps().into_iter().collect::<Vec<_>>();
    add_script_deps(&mut cell_deps, &matched.vault)?;
    add_default_lock_dep(&mut cell_deps, &rpc, &executor_lock)?;

    let mut witnesses = base.witnesses().into_iter().collect::<Vec<_>>();
    while witnesses.len() < base_input_len {
        witnesses.push(packed::Bytes::default());
    }
    witnesses.push(packed::Bytes::default());
    witnesses.push(secp_placeholder_witness());
    witnesses.push(packed::Bytes::default());

    let tx = base
        .as_advanced_builder()
        .set_cell_deps(cell_deps)
        .set_inputs(inputs)
        .set_outputs(outputs)
        .set_outputs_data(outputs_data)
        .set_witnesses(witnesses)
        .build();
    let tx_hash = tx.hash().to_string();
    let transaction: JsonTransaction = tx.data().into();
    Ok(BuiltFiberFundingTx {
        transaction: serde_json::to_value(transaction)?,
        funding_out_point: format!("{}#0x0", tx_hash),
        tx_hash,
        request_id: matched.request.id,
    })
}

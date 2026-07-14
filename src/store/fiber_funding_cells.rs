use anyhow::{Result, anyhow};
use ckb_sdk::{
    CkbRpcClient,
    constants::SIGHASH_TYPE_HASH,
    rpc::ckb_indexer::{Order, SearchKey},
    traits::{CellDepResolver, CellQueryOptions, DefaultCellDepResolver, ValueRangeOption},
};
use ckb_types::{
    core::{Capacity, DepType},
    packed::{self, CellDep, CellInput, CellOutput, OutPoint, Script},
    prelude::*,
};

use super::{
    fiber_funding_builder::{FiberFundingBuilderPayload, MatchedFundingRequest},
    fiber_funding_hex::{
        add_u64, cell_input, encode_funding_intent_data, encode_request_data, encode_vault_data,
        padded_id, read_u64, script_code_hash, script_dep_out_point, script_from_address, sub_u64,
    },
};
use crate::domain::VaultConfig;

const SHANNONS_PER_CKB: u64 = 100_000_000;
const REQUEST_STATUS_DEPLOYED: u8 = 2;
const FUNDING_STATUS_READY: u8 = 0;
const BUILDER_FEE_SHANNONS: u64 = 1_000_000;

pub(super) struct LiveInput {
    pub input: CellInput,
    pub output: CellOutput,
    pub data: packed::Bytes,
}

pub(super) struct TxOutput {
    pub output: CellOutput,
    pub data: packed::Bytes,
}

pub(super) fn live_cell(rpc: &CkbRpcClient, out_point: OutPoint) -> Result<LiveInput> {
    let cell = rpc
        .get_live_cell(out_point.clone().into(), true)?
        .cell
        .ok_or_else(|| anyhow!("CKB live cell was not found"))?;
    let data = cell
        .data
        .map(|data| data.content.into_bytes().to_vec())
        .unwrap_or_default();
    Ok(LiveInput {
        input: cell_input(out_point),
        output: cell.output.into(),
        data: data.pack(),
    })
}

pub(super) fn executor_cell(rpc_url: &str, address: &str) -> Result<LiveInput> {
    let lock = script_from_address(address)?;
    let mut query = CellQueryOptions::new_lock(lock);
    query.with_data = Some(true);
    query.data_len_range = Some(ValueRangeOption::new_exact(0));
    query.capacity_range = Some(ValueRangeOption::new_min(62 * SHANNONS_PER_CKB));
    let rpc = CkbRpcClient::new(rpc_url);
    let page = rpc.get_cells(SearchKey::from(query), Order::Desc, 10u32.into(), None)?;
    let cell = page
        .objects
        .into_iter()
        .max_by_key(|cell| u64::from(cell.output.capacity))
        .ok_or_else(|| anyhow!("executor wallet has no spendable CKB cell for builder fees"))?;
    Ok(LiveInput {
        input: cell_input(cell.out_point.into()),
        output: cell.output.into(),
        data: cell
            .output_data
            .map(|data| data.into_bytes().to_vec())
            .unwrap_or_default()
            .pack(),
    })
}

pub(super) fn funding_cell(
    payload: &FiberFundingBuilderPayload,
    funding_lock: Script,
) -> Result<TxOutput> {
    let capacity = payload
        .request
        .local_amount
        .checked_add(u128::from(payload.request.local_reserved_ckb_amount))
        .ok_or_else(|| anyhow!("Fiber funding amount exceeds u128 shannon range"))?;
    let capacity = u64::try_from(capacity)
        .map_err(|_| anyhow!("Fiber funding amount exceeds u64 shannon range"))?;
    Ok(TxOutput {
        output: CellOutput::new_builder()
            .capacity(capacity)
            .lock(funding_lock)
            .build(),
        data: packed::Bytes::default(),
    })
}

pub(super) fn next_vault_cell(input: &LiveInput, funding_shannons: u64) -> Result<TxOutput> {
    let data = input.data.raw_data();
    if data.len() != 33 || data[0] != 1 {
        return Err(anyhow!("live vault cell data is invalid"));
    }
    let total = read_u64(&data, 1)?;
    let reserved = read_u64(&data, 9)?;
    let deployed = read_u64(&data, 17)?;
    let fee_balance = read_u64(&data, 25)?;
    let amount = funding_shannons / SHANNONS_PER_CKB;
    let output = input
        .output
        .clone()
        .as_builder()
        .capacity(sub_u64(
            input.output.capacity().unpack(),
            funding_shannons,
            "vault capacity",
        )?)
        .build();
    let min = output.occupied_capacity(Capacity::bytes(33)?)?.as_u64();
    let output_capacity: u64 = output.capacity().unpack();
    if output_capacity < min {
        return Err(anyhow!(
            "vault output capacity would fall below minimum cell capacity"
        ));
    }
    Ok(TxOutput {
        output,
        data: encode_vault_data(
            total,
            sub_u64(reserved, amount, "vault reserved")?,
            add_u64(deployed, amount, "vault deployed")?,
            fee_balance,
        ),
    })
}

pub(super) fn next_request_cell(input: &LiveInput) -> Result<TxOutput> {
    let data = input.data.raw_data();
    if data.len() != 26 || data[0] != 1 {
        return Err(anyhow!("capacity request cell data is invalid"));
    }
    Ok(TxOutput {
        output: input.output.clone(),
        data: encode_request_data(
            REQUEST_STATUS_DEPLOYED,
            read_u64(&data, 2)?,
            read_u64(&data, 10)?,
            read_u64(&data, 18)?,
        ),
    })
}

pub(super) fn funding_intent_cell(
    matched: &MatchedFundingRequest,
    vault_type: &Script,
    request_type: &Script,
    executor_lock: &Script,
    funding_lock: &Script,
) -> Result<TxOutput> {
    let args = [
        vault_type.calc_script_hash().raw_data().to_vec(),
        request_type.code_hash().raw_data().to_vec(),
        executor_lock.calc_script_hash().raw_data().to_vec(),
        funding_lock.calc_script_hash().raw_data().to_vec(),
        padded_id(&matched.request.id),
    ]
    .concat();
    let type_script = Script::new_builder()
        .code_hash(script_code_hash(
            &matched.vault.scripts.funding_intent_type_code_hash,
            "LIQUIDLANE_FUNDING_INTENT_TYPE_CODE_HASH",
        )?)
        .hash_type(ckb_types::core::ScriptHashType::Data1)
        .args(args.pack())
        .build();
    let data = encode_funding_intent_data(FUNDING_STATUS_READY, matched.request.amount);
    let output = CellOutput::new_builder()
        .lock(executor_lock.clone())
        .type_(Some(type_script).pack())
        .build();
    Ok(TxOutput {
        output: output
            .clone()
            .as_builder()
            .capacity(
                output
                    .occupied_capacity(Capacity::bytes(data.len())?)?
                    .pack(),
            )
            .build(),
        data,
    })
}

pub(super) fn executor_change_cell(
    executor: &LiveInput,
    funding_intent: &TxOutput,
) -> Result<Option<TxOutput>> {
    let input_capacity: u64 = executor.output.capacity().unpack();
    let used = add_u64(
        funding_intent.output.capacity().unpack(),
        BUILDER_FEE_SHANNONS,
        "executor fee",
    )?;
    let Some(change_capacity) = input_capacity.checked_sub(used) else {
        return Err(anyhow!(
            "executor cell cannot cover funding-intent capacity and fees"
        ));
    };
    let output = CellOutput::new_builder()
        .lock(executor.output.lock())
        .capacity(change_capacity)
        .build();
    let min = output.occupied_capacity(Capacity::zero())?.as_u64();
    if change_capacity < min {
        return Ok(None);
    }
    Ok(Some(TxOutput {
        output,
        data: packed::Bytes::default(),
    }))
}

pub(super) fn add_script_deps(cell_deps: &mut Vec<CellDep>, vault: &VaultConfig) -> Result<()> {
    for (out_point, label) in [
        (
            &vault.scripts.vault_lock_out_point,
            "LIQUIDLANE_VAULT_LOCK_OUT_POINT",
        ),
        (
            &vault.scripts.vault_type_out_point,
            "LIQUIDLANE_VAULT_TYPE_OUT_POINT",
        ),
        (
            &vault.scripts.request_type_out_point,
            "LIQUIDLANE_REQUEST_TYPE_OUT_POINT",
        ),
        (
            &vault.scripts.funding_intent_type_out_point,
            "LIQUIDLANE_FUNDING_INTENT_TYPE_OUT_POINT",
        ),
    ] {
        push_dep(
            cell_deps,
            script_dep_out_point(out_point, label)?,
            DepType::Code,
        );
    }
    Ok(())
}

pub(super) fn add_default_lock_dep(
    cell_deps: &mut Vec<CellDep>,
    rpc: &CkbRpcClient,
    lock: &Script,
) -> Result<()> {
    if lock.code_hash().as_slice() != SIGHASH_TYPE_HASH.as_bytes() {
        return Ok(());
    }
    let genesis = rpc
        .get_block_by_number(0u64.into())?
        .ok_or_else(|| anyhow!("CKB genesis block unavailable for default lock dep"))?;
    let resolver = DefaultCellDepResolver::from_genesis(&genesis.into())
        .map_err(|err| anyhow!("failed to resolve CKB default lock deps from genesis: {err}"))?;
    let dep = resolver
        .resolve(lock)
        .ok_or_else(|| anyhow!("failed to resolve default sighash lock cell dep"))?;
    push_dep(cell_deps, dep.out_point(), DepType::DepGroup);
    Ok(())
}

fn push_dep(cell_deps: &mut Vec<CellDep>, out_point: OutPoint, dep_type: DepType) {
    if cell_deps.iter().any(|dep| dep.out_point() == out_point) {
        return;
    }
    cell_deps.push(
        CellDep::new_builder()
            .out_point(out_point)
            .dep_type(dep_type)
            .build(),
    );
}

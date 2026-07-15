use anyhow::{Result, anyhow};
use ckb_sdk::{
    CkbRpcClient,
    constants::SIGHASH_TYPE_HASH,
    traits::{CellDepResolver, DefaultCellDepResolver},
};
use ckb_types::{
    core::{Capacity, DepType},
    packed::{self, CellDep, CellInput, CellOutput, OutPoint, Script},
    prelude::*,
};

use super::{
    fiber_funding_builder::FiberFundingBuilderPayload,
    fiber_funding_hex::{
        add_u64, cell_input, encode_request_data, encode_vault_data, read_u64,
        script_dep_out_point, sub_u64,
    },
};
use crate::domain::VaultConfig;

const SHANNONS_PER_CKB: u64 = 100_000_000;
const REQUEST_STATUS_DEPLOYED: u8 = 2;

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

pub(super) fn next_request_cell(input: &LiveInput, fee_shannons: u64) -> Result<TxOutput> {
    let data = input.data.raw_data();
    if data.len() != 26 || data[0] != 1 {
        return Err(anyhow!("capacity request cell data is invalid"));
    }
    let output = input
        .output
        .clone()
        .as_builder()
        .capacity(sub_u64(
            input.output.capacity().unpack(),
            fee_shannons,
            "request funding fee",
        )?)
        .build();
    let min = output.occupied_capacity(Capacity::bytes(26)?)?.as_u64();
    let output_capacity: u64 = output.capacity().unpack();
    if output_capacity < min {
        return Err(anyhow!(
            "request output capacity would fall below minimum cell capacity"
        ));
    }
    Ok(TxOutput {
        output,
        data: encode_request_data(
            REQUEST_STATUS_DEPLOYED,
            read_u64(&data, 2)?,
            read_u64(&data, 10)?,
            read_u64(&data, 18)?,
        ),
    })
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

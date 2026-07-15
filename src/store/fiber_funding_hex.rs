use std::str::FromStr;

use anyhow::{Context, Result, anyhow};
use ckb_sdk::Address;
use ckb_types::{
    H256,
    packed::{self, CellInput, OutPoint, Script, WitnessArgs},
    prelude::*,
};

pub(super) fn parse_out_point(value: &str) -> Result<OutPoint> {
    let (tx_hash, index) = value
        .split_once('#')
        .ok_or_else(|| anyhow!("out-point must be tx_hash#index"))?;
    Ok(OutPoint::new_builder()
        .tx_hash(h256(tx_hash)?.pack())
        .index(hex_u32(index)?)
        .build())
}

pub(super) fn cell_input(out_point: OutPoint) -> CellInput {
    CellInput::new_builder().previous_output(out_point).build()
}

pub(super) fn script_from_address(address: &str) -> Result<Script> {
    let address =
        Address::from_str(address).map_err(|err| anyhow!("invalid CKB address: {err}"))?;
    Ok((&address).into())
}

pub(super) fn packed_script_entity_hex(value: &str) -> Result<Script> {
    let bytes = hex_bytes(value)?;
    Script::from_slice(&bytes).map_err(|err| anyhow!("invalid packed CKB script: {err}"))
}

pub(super) fn script_dep_out_point(value: &Option<String>, label: &str) -> Result<OutPoint> {
    parse_out_point(
        value
            .as_deref()
            .ok_or_else(|| anyhow!("{label} is missing"))?,
    )
}

pub(super) fn encode_vault_data(
    total: u64,
    reserved: u64,
    deployed: u64,
    fee_balance: u64,
) -> packed::Bytes {
    let mut data = Vec::with_capacity(33);
    data.push(1);
    data.extend(total.to_le_bytes());
    data.extend(reserved.to_le_bytes());
    data.extend(deployed.to_le_bytes());
    data.extend(fee_balance.to_le_bytes());
    data.pack()
}

pub(super) fn encode_request_data(
    status: u8,
    amount: u64,
    lease_fee: u64,
    expiry: u64,
) -> packed::Bytes {
    let mut data = Vec::with_capacity(26);
    data.push(1);
    data.push(status);
    data.extend(amount.to_le_bytes());
    data.extend(lease_fee.to_le_bytes());
    data.extend(expiry.to_le_bytes());
    data.pack()
}

pub(super) fn read_u64(data: &[u8], offset: usize) -> Result<u64> {
    let mut raw = [0u8; 8];
    raw.copy_from_slice(
        data.get(offset..offset + 8)
            .ok_or_else(|| anyhow!("u64 field missing"))?,
    );
    Ok(u64::from_le_bytes(raw))
}

pub(super) fn add_u64(left: u64, right: u64, label: &str) -> Result<u64> {
    left.checked_add(right)
        .ok_or_else(|| anyhow!("{label} overflow"))
}

pub(super) fn sub_u64(left: u64, right: u64, label: &str) -> Result<u64> {
    left.checked_sub(right)
        .ok_or_else(|| anyhow!("{label} underflow"))
}

pub(super) fn secp_placeholder_witness() -> packed::Bytes {
    let lock: packed::Bytes = ckb_types::bytes::Bytes::from(vec![0u8; 65]).pack();
    WitnessArgs::new_builder()
        .lock(Some(lock))
        .build()
        .as_bytes()
        .pack()
}

pub(super) fn hex_bytes(value: &str) -> Result<Vec<u8>> {
    let value = value.trim_start_matches("0x");
    if value.len() % 2 != 0 {
        return Err(anyhow!("hex data must have even length"));
    }
    (0..value.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&value[index..index + 2], 16).context("invalid hex data"))
        .collect()
}

fn h256(value: &str) -> Result<H256> {
    H256::from_str(value.trim_start_matches("0x")).context("expected 32-byte hex hash")
}

fn hex_u32(value: &str) -> Result<u32> {
    u32::from_str_radix(value.trim_start_matches("0x"), 16)
        .with_context(|| format!("invalid hex index {value}"))
}

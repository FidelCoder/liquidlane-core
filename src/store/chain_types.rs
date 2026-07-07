use std::str::FromStr;

use anyhow::{Context, Result, anyhow};
use ckb_sdk::Address;
use ckb_types::{H256, core::ScriptHashType, packed::Script, prelude::*};
use serde_json::Value;

const VAULT_DATA_LEN: usize = 33;
const RECEIPT_DATA_LEN: usize = 41;
const REQUEST_DATA_LEN: usize = 26;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ChainScript {
    pub code_hash: String,
    pub hash_type: String,
    pub args: String,
}

#[derive(Clone, Debug)]
pub(super) struct ChainOutput {
    pub lock: ChainScript,
    pub type_script: Option<ChainScript>,
    pub capacity: u128,
    pub data: Vec<u8>,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct VaultData {
    pub total: u64,
    pub reserved: u64,
    pub deployed: u64,
    pub fee_balance: u64,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct ReceiptData {
    pub supplied: u64,
    pub available: u64,
    pub reserved: u64,
    pub deployed: u64,
    pub claimed: u64,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct RequestData {
    pub status: u8,
    pub amount: u64,
    pub lease_fee: u64,
    pub expiry: u64,
}

pub(super) fn outputs(transaction: &Value) -> Result<Vec<ChainOutput>> {
    let output_values = array(transaction, "outputs")?;
    let data_values = array(transaction, "outputs_data")?;
    if output_values.len() != data_values.len() {
        return Err(anyhow!(
            "transaction outputs and outputs_data lengths differ"
        ));
    }
    output_values
        .iter()
        .zip(data_values.iter())
        .map(|(output, data)| parse_output(output, data))
        .collect()
}

pub(super) fn output_at(transaction: &Value, index: usize) -> Result<ChainOutput> {
    let output = array(transaction, "outputs")?
        .get(index)
        .ok_or_else(|| anyhow!("previous vault output index is out of range"))?;
    let data = array(transaction, "outputs_data")?
        .get(index)
        .ok_or_else(|| anyhow!("previous vault data index is out of range"))?;
    parse_output(output, data)
}

pub(super) fn parse_vault_data(data: &[u8]) -> Result<VaultData> {
    if data.len() != VAULT_DATA_LEN || data[0] != 1 {
        return Err(anyhow!("vault cell data is invalid"));
    }
    Ok(VaultData {
        total: le_u64(data, 1)?,
        reserved: le_u64(data, 9)?,
        deployed: le_u64(data, 17)?,
        fee_balance: le_u64(data, 25)?,
    })
}

pub(super) fn parse_receipt_data(data: &[u8]) -> Result<ReceiptData> {
    if data.len() != RECEIPT_DATA_LEN || data[0] != 1 {
        return Err(anyhow!("LP receipt data is invalid"));
    }
    Ok(ReceiptData {
        supplied: le_u64(data, 1)?,
        available: le_u64(data, 9)?,
        reserved: le_u64(data, 17)?,
        deployed: le_u64(data, 25)?,
        claimed: le_u64(data, 33)?,
    })
}

pub(super) fn parse_request_data(data: &[u8]) -> Result<RequestData> {
    if data.len() != REQUEST_DATA_LEN || data[0] != 1 {
        return Err(anyhow!("capacity request cell data is invalid"));
    }
    let request = RequestData {
        status: data[1],
        amount: le_u64(data, 2)?,
        lease_fee: le_u64(data, 10)?,
        expiry: le_u64(data, 18)?,
    };
    if request.status > 3 || request.amount == 0 || request.lease_fee == 0 || request.expiry == 0 {
        return Err(anyhow!("capacity request cell data is invalid"));
    }
    Ok(request)
}

pub(super) fn script_from_address(address: &str) -> Result<ChainScript> {
    let address =
        Address::from_str(address).map_err(|err| anyhow!("invalid CKB address: {err}"))?;
    let script: Script = (&address).into();
    script_from_packed(&script)
}

pub(super) fn script_hash(script: &ChainScript) -> Result<String> {
    Ok(hex(packed_script(script)?.calc_script_hash().as_slice()))
}

pub(super) fn required_hash(value: Option<&str>, key: &str) -> Result<String> {
    let value = value
        .ok_or_else(|| anyhow!("{key} is missing"))?
        .to_ascii_lowercase();
    if value.len() == 66 && value.starts_with("0x") {
        Ok(value)
    } else {
        Err(anyhow!("{key} must be a 0x-prefixed 32-byte hash"))
    }
}

pub(super) fn type_code_matches(script: &Option<ChainScript>, code_hash: &str) -> bool {
    script
        .as_ref()
        .map(|script| script.code_hash.eq_ignore_ascii_case(code_hash))
        .unwrap_or(false)
}

pub(super) fn array<'a>(value: &'a Value, key: &str) -> Result<&'a Vec<Value>> {
    value
        .get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("transaction {key} must be an array"))
}

pub(super) fn string_field<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("transaction {key} must be a string"))
}

pub(super) fn hex_index(value: &str) -> Result<usize> {
    usize::from_str_radix(value.trim_start_matches("0x"), 16)
        .with_context(|| format!("invalid out-point index {value}"))
}

fn parse_output(output: &Value, data: &Value) -> Result<ChainOutput> {
    Ok(ChainOutput {
        lock: parse_script(
            output
                .get("lock")
                .ok_or_else(|| anyhow!("output lock is missing"))?,
        )?,
        type_script: output
            .get("type")
            .filter(|value| !value.is_null())
            .map(parse_script)
            .transpose()?,
        capacity: hex_u128(string_field(output, "capacity")?)?,
        data: hex_bytes(data.as_str().unwrap_or("0x"))?,
    })
}

fn script_from_packed(script: &Script) -> Result<ChainScript> {
    let hash_type: ScriptHashType = script
        .hash_type()
        .try_into()
        .map_err(|_| anyhow!("unsupported script hash type"))?;
    Ok(ChainScript {
        code_hash: hex(script.code_hash().as_slice()),
        hash_type: hash_type_name(hash_type).to_string(),
        args: hex(script.args().raw_data().as_ref()),
    })
}

fn packed_script(script: &ChainScript) -> Result<Script> {
    Ok(Script::new_builder()
        .code_hash(parse_h256(&script.code_hash)?.pack())
        .hash_type(parse_hash_type(&script.hash_type)?)
        .args(hex_bytes(&script.args)?.pack())
        .build())
}

fn parse_h256(value: &str) -> Result<H256> {
    H256::from_str(value.trim_start_matches("0x")).context("expected 32-byte hex hash")
}

fn parse_hash_type(value: &str) -> Result<ScriptHashType> {
    match value {
        "data" => Ok(ScriptHashType::Data),
        "data1" => Ok(ScriptHashType::Data1),
        "data2" => Ok(ScriptHashType::Data2),
        "type" => Ok(ScriptHashType::Type),
        _ => Err(anyhow!("unsupported script hash type {value}")),
    }
}

fn parse_script(value: &Value) -> Result<ChainScript> {
    Ok(ChainScript {
        code_hash: string_field(value, "code_hash")?.to_ascii_lowercase(),
        hash_type: string_field(value, "hash_type")?.to_ascii_lowercase(),
        args: string_field(value, "args")?.to_ascii_lowercase(),
    })
}

fn hex_u128(value: &str) -> Result<u128> {
    u128::from_str_radix(value.trim_start_matches("0x"), 16)
        .with_context(|| format!("invalid hex integer {value}"))
}

fn hex_bytes(value: &str) -> Result<Vec<u8>> {
    let value = value.trim_start_matches("0x");
    if value.len() % 2 != 0 {
        return Err(anyhow!("hex data must have even length"));
    }
    (0..value.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&value[index..index + 2], 16).context("invalid hex data"))
        .collect()
}

fn le_u64(data: &[u8], offset: usize) -> Result<u64> {
    let mut raw = [0u8; 8];
    raw.copy_from_slice(
        data.get(offset..offset + 8)
            .ok_or_else(|| anyhow!("u64 out of range"))?,
    );
    Ok(u64::from_le_bytes(raw))
}

fn hash_type_name(hash_type: ScriptHashType) -> &'static str {
    match hash_type {
        ScriptHashType::Data => "data",
        ScriptHashType::Data1 => "data1",
        ScriptHashType::Data2 => "data2",
        ScriptHashType::Type => "type",
        _ => "unknown",
    }
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

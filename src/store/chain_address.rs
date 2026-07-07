use std::str::FromStr;

use anyhow::{Context, Result, anyhow};
use ckb_sdk::{Address, AddressPayload, NetworkType};
use ckb_types::{H256, core::ScriptHashType, packed::Script, prelude::*};
use serde_json::Value;

use super::chain_types::{ChainScript, string_field};

pub(super) fn script_from_json(value: &Value) -> Result<ChainScript> {
    Ok(ChainScript {
        code_hash: string_field(value, "code_hash")?.to_ascii_lowercase(),
        hash_type: string_field(value, "hash_type")?.to_ascii_lowercase(),
        args: string_field(value, "args")?.to_ascii_lowercase(),
    })
}

pub(super) fn address_from_script(script: &ChainScript, network: &str) -> Result<String> {
    let network = match network.trim().to_ascii_lowercase().as_str() {
        "mainnet" | "ckb-mainnet" => NetworkType::Mainnet,
        _ => NetworkType::Testnet,
    };
    Ok(Address::new(network, AddressPayload::from(packed_script(script)?), true).to_string())
}

fn packed_script(script: &ChainScript) -> Result<Script> {
    Ok(Script::new_builder()
        .code_hash(parse_h256(&script.code_hash)?.pack())
        .hash_type(parse_hash_type(&script.hash_type)? as u8)
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

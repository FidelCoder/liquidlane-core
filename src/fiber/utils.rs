use anyhow::{Result, anyhow};
use serde_json::{Value, json};

use crate::domain::CkbScript;

const SHANNONS_PER_CKB: u128 = 100_000_000;

pub(super) fn non_empty(value: String) -> Option<String> {
    let value = value.trim().to_string();
    (!value.is_empty()).then_some(value)
}

pub(super) fn funding_amount_hex(asset: &str, amount: u64) -> Result<String> {
    let amount = u128::from(amount);
    let funding_amount = if asset == "CKB" {
        amount
            .checked_mul(SHANNONS_PER_CKB)
            .ok_or_else(|| anyhow!("fiber CKB funding amount overflow"))?
    } else {
        amount
    };
    Ok(format!("0x{funding_amount:x}"))
}

pub(super) fn string_field(value: &Value, field: &str) -> Option<String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(ToString::to_string)
}

pub(super) fn script_to_value(script: &CkbScript) -> Value {
    json!({
        "code_hash": script.code_hash,
        "hash_type": script.hash_type,
        "args": script.args,
    })
}

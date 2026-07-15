use anyhow::{Result, anyhow};
use serde_json::Value;

use super::chain_types::{outputs, script_from_address};
use crate::domain::LiquidityRequest;

pub(super) fn require_receiver_reserve_payment(
    transaction: &Value,
    request: &LiquidityRequest,
) -> Result<()> {
    let receiver_address = request
        .receiver_ckb_address
        .as_deref()
        .ok_or_else(|| anyhow!("request receiver CKB address is missing"))?;
    let receiver_lock = script_from_address(receiver_address)?;
    let expected_capacity = u128::from(request.receiver_reserve_payment)
        .checked_mul(100_000_000)
        .ok_or_else(|| anyhow!("receiver reserve payment overflow"))?;
    let matches = outputs(transaction)?
        .into_iter()
        .filter(|output| output.lock == receiver_lock && output.type_script.is_none())
        .collect::<Vec<_>>();
    if matches.len() == 1 && matches[0].capacity == expected_capacity && matches[0].data.is_empty()
    {
        return Ok(());
    }
    Err(anyhow!(
        "request transaction must pay exactly {} CKB to the declared Fiber receiver reserve address",
        request.receiver_reserve_payment
    ))
}

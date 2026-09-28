use super::{
    Market, crypto,
    model::{Order, SHANNONS, TESTNET_GENESIS},
};
use anyhow::{Context, Result, ensure};
use ckb_types::packed;
use serde_json::{Value, json};

pub async fn rpc(
    client: &reqwest::Client,
    url: &str,
    method: &str,
    params: Value,
) -> Result<Value> {
    let response: Value = client
        .post(url)
        .json(&json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    ensure!(
        response.get("error").is_none(),
        "RPC {method}: {}",
        response.get("error").unwrap_or(&Value::Null)
    );
    response
        .get("result")
        .cloned()
        .context("RPC response is missing result")
}
pub async fn verify_network(client: &reqwest::Client, url: &str) -> Result<()> {
    ensure!(
        rpc(client, url, "get_block_hash", json!(["0x0"])).await? == TESTNET_GENESIS,
        "RPC must be CKB testnet"
    );
    Ok(())
}
pub async fn confirmed(state: &Market, hash: &str) -> Result<Value> {
    ensure!(crypto::bytes(hash)?.len() == 32, "invalid transaction hash");
    let value = rpc(
        &state.client,
        &state.config.ckb_rpc,
        "get_transaction",
        json!([hash]),
    )
    .await?;
    ensure!(
        value["tx_status"]["status"] == "committed",
        "transaction is not confirmed on CKB testnet"
    );
    value
        .get("transaction")
        .cloned()
        .filter(|v| v.is_object())
        .context("transaction body is unavailable")
}
pub async fn verify_funding(state: &Market, order: &Order, outpoint: &str) -> Result<()> {
    let (hash, index) = parse_outpoint(outpoint)?;
    let tx = confirmed(state, &hash).await?;
    let output = tx["outputs"]
        .get(index)
        .context("funding output does not exist")?;
    ensure!(
        output["type"].is_null(),
        "pilot requires native CKB funding"
    );
    let capacity = number(&output["capacity"])?;
    let expected = order
        .quote
        .funding_ckb
        .checked_mul(SHANNONS)
        .context("funding overflow")?;
    ensure!(
        capacity >= expected,
        "funding output is below the signed quote"
    );
    let lock = output["lock"]["code_hash"]
        .as_str()
        .context("missing funding lock")?;
    // Fiber v0.9.0 testnet funding lock, pinned to the deployed native script.
    ensure!(
        lock == super::FIBER_FUNDING_CODE_HASH,
        "unexpected Fiber funding lock"
    );
    ensure!(
        output["lock"]["hash_type"] == "type",
        "unexpected Fiber funding hash type"
    );
    ensure!(
        crypto::bytes(
            output["lock"]["args"]
                .as_str()
                .context("missing funding args")?
        )?
        .len()
            == 20,
        "unexpected Fiber funding lock arguments"
    );
    Ok(())
}
pub async fn verify_fee(state: &Market, order: &Order, hash: &str) -> Result<()> {
    let status = rpc(
        &state.client,
        &state.config.ckb_rpc,
        "get_transaction",
        json!([hash]),
    )
    .await?;
    let block_hash = status["tx_status"]["block_hash"]
        .as_str()
        .context("fee transaction is not yet confirmed")?;
    let header = rpc(
        &state.client,
        &state.config.ckb_rpc,
        "get_header",
        json!([block_hash]),
    )
    .await?;
    ensure!(
        number(&header["number"])?
            > order
                .delivery_block_number
                .context("delivery chain anchor missing")?,
        "fee transaction must confirm after this order's verified delivery"
    );
    let tx = confirmed(state, hash).await?;
    let recipient = script_json(&crypto::address_script(&order.quote.fee_recipient)?);
    let merchant = script_json(&crypto::address_script(&order.owner)?);
    let expected = order
        .quote
        .opening_fee_ckb
        .checked_mul(SHANNONS)
        .context("fee overflow")?;
    let outputs = tx["outputs"].as_array().context("missing outputs")?;
    let matching: Vec<_> = outputs
        .iter()
        .enumerate()
        .filter(|(_, o)| o["lock"] == recipient && o["type"].is_null())
        .collect();
    ensure!(
        matching.len() == 1 && number(&matching[0].1["capacity"])? == expected,
        "fee output does not match the accepted quote"
    );
    ensure!(
        tx["outputs_data"][matching[0].0] == "0x",
        "fee output must have empty data"
    );
    let mut merchant_signed = false;
    for input in tx["inputs"].as_array().context("missing inputs")? {
        let previous = &input["previous_output"];
        let previous_tx = confirmed(
            state,
            previous["tx_hash"].as_str().context("missing input hash")?,
        )
        .await?;
        let index = number(&previous["index"])? as usize;
        if previous_tx["outputs"][index]["lock"] == merchant {
            merchant_signed = true;
        }
    }
    ensure!(
        merchant_signed,
        "fee transaction must spend a cell from the authenticated merchant"
    );
    Ok(())
}
pub async fn verify_live_funding(state: &Market, outpoint: &str) -> Result<()> {
    let (hash, index) = parse_outpoint(outpoint)?;
    let result = rpc(
        &state.client,
        &state.config.ckb_rpc,
        "get_live_cell",
        json!([{"tx_hash":hash,"index":format!("0x{index:x}")},false]),
    )
    .await?;
    ensure!(
        result["status"] == "live",
        "channel funding is spent or unavailable; delivery cannot be verified"
    );
    Ok(())
}
pub fn script_json(script: &packed::Script) -> Value {
    let script: ckb_jsonrpc_types::Script = script.clone().into();
    serde_json::to_value(script).expect("CKB script serializes")
}
pub fn number(value: &Value) -> Result<u64> {
    if let Some(value) = value.as_u64() {
        return Ok(value);
    }
    let value = value.as_str().context("numeric field missing")?;
    Ok(if let Some(hex) = value.strip_prefix("0x") {
        u64::from_str_radix(hex, 16)?
    } else {
        value.parse()?
    })
}
pub fn parse_outpoint(value: &str) -> Result<(String, usize)> {
    if let Some((hash, index)) = value.split_once('#') {
        ensure!(crypto::bytes(hash)?.len() == 32, "invalid funding outpoint");
        return Ok((hash.into(), number(&Value::String(index.into()))? as usize));
    }
    let bytes = crypto::bytes(value)?;
    ensure!(
        bytes.len() == 36,
        "expected a 36-byte packed funding outpoint"
    );
    Ok((
        format!("0x{}", hex::encode(&bytes[..32])),
        u32::from_le_bytes(bytes[32..].try_into()?) as usize,
    ))
}

pub fn canonical_outpoint(value: &str) -> Result<String> {
    let (hash, index) = parse_outpoint(value)?;
    ensure!(index <= u32::MAX as usize, "invalid output index");
    Ok(format!("0x{}#{index}", hex::encode(crypto::bytes(&hash)?)))
}

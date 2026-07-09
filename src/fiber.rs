use anyhow::{Result, anyhow};
use reqwest::Client;
use serde::Deserialize;
use serde_json::{Map, Value, json};

const SHANNONS_PER_CKB: u128 = 100_000_000;

use crate::domain::{CkbScript, LiquidityRequest};

#[derive(Clone)]
pub struct FiberClient {
    rpc_url: Option<String>,
    auth_token: Option<String>,
    http: Client,
}

#[derive(Clone, Debug)]
pub struct FiberOpenOutcome {
    pub rpc_submitted: bool,
    pub temporary_channel_id: Option<String>,
    pub channel_id: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Deserialize)]
struct JsonRpcEnvelope {
    result: Option<Value>,
    error: Option<JsonRpcError>,
}

#[derive(Debug, Deserialize)]
struct JsonRpcError {
    code: i64,
    message: String,
    #[allow(dead_code)]
    data: Option<Value>,
}

impl FiberClient {
    pub fn new(rpc_url: Option<String>, auth_token: Option<String>) -> Self {
        Self {
            rpc_url: rpc_url.and_then(non_empty),
            auth_token: auth_token.and_then(non_empty),
            http: Client::new(),
        }
    }

    #[cfg(test)]
    pub fn disabled() -> Self {
        Self::new(None, None)
    }

    pub fn is_configured(&self) -> bool {
        self.rpc_url.is_some()
    }

    pub async fn open_channel(&self, request: &LiquidityRequest) -> Result<FiberOpenOutcome> {
        let rpc_url = self.rpc_url()?;
        let peer_pubkey = request.fiber_peer_pubkey.as_deref().ok_or_else(|| {
            anyhow!("fiber_peer_pubkey is required to submit open_channel to Fiber RPC")
        })?;

        if request.asset != "CKB" && request.funding_udt_type_script.is_none() {
            return Err(anyhow!(
                "funding_udt_type_script is required to open a {} Fiber UDT channel",
                request.asset
            ));
        }

        self.connect_peer(rpc_url, peer_pubkey).await?;

        let mut params = Map::new();
        params.insert("pubkey".to_string(), Value::String(peer_pubkey.to_string()));
        params.insert(
            "funding_amount".to_string(),
            Value::String(funding_amount_hex(&request.asset, request.amount)?),
        );
        params.insert("public".to_string(), json!(request.public_channel));
        params.insert("one_way".to_string(), json!(false));
        if let Some(script) = request.funding_udt_type_script.as_ref() {
            params.insert(
                "funding_udt_type_script".to_string(),
                script_to_value(script),
            );
        }

        let result = self
            .rpc_call(
                rpc_url,
                "open_channel",
                request.id.to_string(),
                Value::Object(params),
            )
            .await?;

        Ok(FiberOpenOutcome {
            rpc_submitted: true,
            temporary_channel_id: string_field(&result, "temporary_channel_id"),
            channel_id: string_field(&result, "channel_id"),
            note: Some(
                "Fiber connect_peer and open_channel submitted to configured node.".to_string(),
            ),
        })
    }

    fn rpc_url(&self) -> Result<&str> {
        let Some(rpc_url) = self.rpc_url.as_deref() else {
            return Err(anyhow!(
                "FIBER_RPC_URL is required before submitting Fiber open_channel"
            ));
        };

        if !(rpc_url.starts_with("http://") || rpc_url.starts_with("https://")) {
            return Err(anyhow!(
                "FIBER_RPC_URL must be an HTTP JSON-RPC endpoint for open_channel submission"
            ));
        }

        Ok(rpc_url)
    }

    async fn connect_peer(&self, rpc_url: &str, peer_pubkey: &str) -> Result<()> {
        let params = json!({
            "pubkey": peer_pubkey,
            "save": true,
        });
        self.rpc_call(
            rpc_url,
            "connect_peer",
            format!("connect-{peer_pubkey}"),
            params,
        )
        .await?;
        Ok(())
    }

    async fn rpc_call(
        &self,
        rpc_url: &str,
        method: &str,
        id: impl Into<String>,
        params: Value,
    ) -> Result<Value> {
        let body = json!({
            "jsonrpc": "2.0",
            "id": id.into(),
            "method": method,
            "params": [params],
        });

        let mut builder = self.http.post(rpc_url).json(&body);
        if let Some(token) = self.auth_token.as_deref() {
            builder = builder.bearer_auth(token);
        }

        let response = builder.send().await?;
        let status = response.status();
        let envelope = response.json::<JsonRpcEnvelope>().await?;
        if let Some(error) = envelope.error {
            return Err(anyhow!(
                "Fiber RPC {method} failed ({}): {}",
                error.code,
                error.message
            ));
        }
        if !status.is_success() {
            return Err(anyhow!("Fiber RPC {method} returned HTTP {status}"));
        }

        Ok(envelope.result.unwrap_or(Value::Null))
    }
}

fn non_empty(value: String) -> Option<String> {
    let value = value.trim().to_string();
    (!value.is_empty()).then_some(value)
}

fn funding_amount_hex(asset: &str, amount: u64) -> Result<String> {
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

fn string_field(value: &Value, field: &str) -> Option<String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(ToString::to_string)
}

fn script_to_value(script: &CkbScript) -> Value {
    json!({
        "code_hash": script.code_hash,
        "hash_type": script.hash_type,
        "args": script.args,
    })
}

#[cfg(test)]
mod tests {
    use super::funding_amount_hex;

    #[test]
    fn converts_ckb_funding_amount_to_shannons_hex() {
        assert_eq!(funding_amount_hex("CKB", 499).unwrap(), "0xb9e459300");
    }

    #[test]
    fn leaves_udt_funding_amount_in_asset_units() {
        assert_eq!(funding_amount_hex("RUSD", 20_000_000).unwrap(), "0x1312d00");
    }
}

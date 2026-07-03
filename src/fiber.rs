use anyhow::{Result, anyhow};
use reqwest::Client;
use serde::Deserialize;
use serde_json::{Map, Value, json};

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
        let Some(rpc_url) = self.rpc_url.as_deref() else {
            return Ok(FiberOpenOutcome {
                rpc_submitted: false,
                temporary_channel_id: None,
                channel_id: None,
                note: Some(
                    "Fiber RPC is not configured; capacity is reserved for an operator node to open."
                        .to_string(),
                ),
            });
        };

        if !(rpc_url.starts_with("http://") || rpc_url.starts_with("https://")) {
            return Err(anyhow!(
                "FIBER_RPC_URL must be an HTTP JSON-RPC endpoint for open_channel submission"
            ));
        }

        let peer_pubkey = request.fiber_peer_pubkey.as_deref().ok_or_else(|| {
            anyhow!("fiber_peer_pubkey is required to submit open_channel to Fiber RPC")
        })?;

        if request.asset != "CKB" && request.funding_udt_type_script.is_none() {
            return Err(anyhow!(
                "funding_udt_type_script is required to open a {} Fiber UDT channel",
                request.asset
            ));
        }

        let mut params = Map::new();
        params.insert("pubkey".to_string(), Value::String(peer_pubkey.to_string()));
        params.insert("funding_amount".to_string(), json!(request.amount));
        params.insert("public".to_string(), json!(request.public_channel));
        params.insert("one_way".to_string(), json!(false));
        if let Some(script) = request.funding_udt_type_script.as_ref() {
            params.insert(
                "funding_udt_type_script".to_string(),
                script_to_value(script),
            );
        }

        let body = json!({
            "jsonrpc": "2.0",
            "id": request.id.to_string(),
            "method": "open_channel",
            "params": Value::Object(params),
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
                "Fiber RPC open_channel failed ({}): {}",
                error.code,
                error.message
            ));
        }
        if !status.is_success() {
            return Err(anyhow!("Fiber RPC returned HTTP {status}"));
        }

        let result = envelope
            .result
            .ok_or_else(|| anyhow!("Fiber RPC open_channel returned no result"))?;

        Ok(FiberOpenOutcome {
            rpc_submitted: true,
            temporary_channel_id: string_field(&result, "temporary_channel_id"),
            channel_id: string_field(&result, "channel_id"),
            note: Some("Fiber open_channel submitted to configured node.".to_string()),
        })
    }
}

fn non_empty(value: String) -> Option<String> {
    let value = value.trim().to_string();
    (!value.is_empty()).then_some(value)
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

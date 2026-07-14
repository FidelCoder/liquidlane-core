use std::time::Duration;

use anyhow::{Result, anyhow};
use reqwest::Client;
use serde::Deserialize;
use serde_json::{Map, Value, json};

const SHANNONS_PER_CKB: u128 = 100_000_000;

use crate::domain::{CkbScript, LiquidityRequest};

mod channel;
mod external;
pub use channel::FiberChannel;
use channel::channel_from_value;
#[allow(unused_imports)]
pub use external::{FiberExternalFundingOutcome, FiberExternalFundingParams};

#[derive(Clone)]
pub struct FiberClient {
    rpc_url: Option<String>,
    auth_token: Option<String>,
    http: Client,
}

#[derive(Clone, Debug)]
pub struct FiberOpenOutcome {
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
        let http = Client::builder()
            .timeout(Duration::from_secs(75))
            .build()
            .unwrap_or_else(|_| Client::new());
        Self {
            rpc_url: rpc_url.and_then(non_empty),
            auth_token: auth_token.and_then(non_empty),
            http,
        }
    }

    #[cfg(test)]
    pub fn disabled() -> Self {
        Self::new(None, None)
    }

    pub fn is_configured(&self) -> bool {
        self.rpc_url.is_some()
    }

    pub async fn list_channels(&self) -> Result<Vec<FiberChannel>> {
        let rpc_url = self.rpc_url()?;
        let mut channels = self
            .list_channels_with_params(rpc_url, json!({ "include_closed": false }))
            .await?;
        channels.extend(
            self.list_channels_with_params(rpc_url, json!({ "only_pending": true }))
                .await?,
        );
        Ok(channels)
    }

    async fn list_channels_with_params(
        &self,
        rpc_url: &str,
        params: Value,
    ) -> Result<Vec<FiberChannel>> {
        let result = self
            .rpc_call_params(
                rpc_url,
                "list_channels",
                "liquidlane-list-channels",
                json!([params]),
            )
            .await?;
        Ok(result
            .get("channels")
            .and_then(Value::as_array)
            .map(|items| items.iter().map(channel_from_value).collect())
            .unwrap_or_default())
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

        self.reject_self_peer(rpc_url, peer_pubkey).await?;
        self.connect_peer(rpc_url, request).await?;

        let mut params = Map::new();
        params.insert("pubkey".to_string(), Value::String(peer_pubkey.to_string()));
        params.insert(
            "funding_amount".to_string(),
            Value::String(funding_amount_hex(&request.asset, request.amount)?),
        );
        params.insert("public".to_string(), json!(false));
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
            temporary_channel_id: string_field(&result, "temporary_channel_id"),
            channel_id: string_field(&result, "channel_id"),
            note: Some(
                "Private one-way Fiber connect_peer and open_channel submitted to configured node."
                    .to_string(),
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

    async fn reject_self_peer(&self, rpc_url: &str, peer_pubkey: &str) -> Result<()> {
        let info = self
            .rpc_call_params(rpc_url, "node_info", "liquidlane-node-info", json!([]))
            .await?;
        let Some(local_pubkey) = string_field(&info, "pubkey") else {
            return Ok(());
        };
        if local_pubkey.eq_ignore_ascii_case(peer_pubkey) {
            return Err(anyhow!(
                "Receiving Fiber pubkey cannot be the operator node pubkey. Use the merchant or receiver Fiber node pubkey."
            ));
        }
        Ok(())
    }

    async fn connect_peer(&self, rpc_url: &str, request: &LiquidityRequest) -> Result<()> {
        let peer_pubkey = request.fiber_peer_pubkey.as_deref().unwrap_or_default();
        let params = if let Some(address) = request.fiber_peer_address.as_deref() {
            json!({ "address": address, "save": true })
        } else {
            json!({ "pubkey": peer_pubkey, "save": true })
        };
        self.rpc_call(
            rpc_url,
            "connect_peer",
            format!("connect-{peer_pubkey}"),
            params,
        )
        .await?;
        self.wait_for_peer(rpc_url, peer_pubkey).await
    }

    async fn wait_for_peer(&self, rpc_url: &str, peer_pubkey: &str) -> Result<()> {
        for _ in 0..20 {
            if self.peer_is_connected(rpc_url, peer_pubkey).await? {
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        Err(anyhow!(
            "Fiber peer {peer_pubkey} is not connected after connect_peer; retry once the peer Init handshake is visible"
        ))
    }

    async fn peer_is_connected(&self, rpc_url: &str, peer_pubkey: &str) -> Result<bool> {
        let result = self
            .rpc_call_params(rpc_url, "list_peers", "liquidlane-list-peers", json!([]))
            .await?;
        Ok(result
            .get("peers")
            .and_then(Value::as_array)
            .is_some_and(|peers| {
                peers.iter().any(|peer| {
                    peer.get("pubkey")
                        .and_then(Value::as_str)
                        .is_some_and(|value| value.eq_ignore_ascii_case(peer_pubkey))
                })
            }))
    }

    async fn rpc_call(
        &self,
        rpc_url: &str,
        method: &str,
        id: impl Into<String>,
        params: Value,
    ) -> Result<Value> {
        self.rpc_call_params(rpc_url, method, id, json!([params]))
            .await
    }

    async fn rpc_call_params(
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
            "params": params,
        });

        let mut builder = self.http.post(rpc_url).json(&body);
        if let Some(token) = self.auth_token.as_deref() {
            builder = builder.bearer_auth(token);
        }

        let response = builder.send().await.map_err(|err| {
            if method == "open_channel_with_external_funding" && err.is_timeout() {
                anyhow!(
                    "Fiber RPC {method} timed out before returning an unsigned external funding transaction. The receiver must accept the channel before LiquidLane can build the vault-funded CKB transaction; Fiber v0.9 external funding currently requires receiver-side accept-channel funding instead of a pure zero-funding one-way accept."
                )
            } else {
                anyhow!("Fiber RPC {method} transport failed: {err}")
            }
        })?;
        let status = response.status();
        let envelope = response.json::<JsonRpcEnvelope>().await.map_err(|err| {
            anyhow!("Fiber RPC {method} returned an invalid JSON-RPC response: {err}")
        })?;
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
mod tests;

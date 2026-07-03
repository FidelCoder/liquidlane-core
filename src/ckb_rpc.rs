use anyhow::{Result, anyhow};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Debug)]
pub struct CkbRpcClient {
    client: Client,
    url: String,
    accept_pending: bool,
}

#[derive(Debug, Deserialize)]
struct RpcResponse<T> {
    result: Option<T>,
    error: Option<RpcError>,
}

#[derive(Debug, Deserialize)]
struct RpcError {
    code: i64,
    message: String,
}

#[derive(Debug, Deserialize)]
struct TransactionResult {
    tx_status: TxStatus,
}

#[derive(Debug, Deserialize)]
struct TxStatus {
    status: String,
    #[serde(default)]
    reason: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct VerifiedCkbTransaction {
    pub tx_hash: String,
    pub status: String,
}

impl CkbRpcClient {
    pub fn new(url: String, accept_pending: bool) -> Self {
        Self {
            client: Client::new(),
            url,
            accept_pending,
        }
    }

    pub async fn verify_transaction(&self, tx_hash: &str) -> Result<VerifiedCkbTransaction> {
        let response = self
            .client
            .post(&self.url)
            .json(&json!({
                "id": 1,
                "jsonrpc": "2.0",
                "method": "get_transaction",
                "params": [tx_hash]
            }))
            .send()
            .await?
            .error_for_status()?
            .json::<RpcResponse<TransactionResult>>()
            .await?;

        if let Some(error) = response.error {
            return Err(anyhow!(
                "CKB RPC get_transaction failed: {} ({})",
                error.message,
                error.code
            ));
        }
        let result = response
            .result
            .ok_or_else(|| anyhow!("CKB transaction was not found on the configured node"))?;
        self.validate_status(tx_hash, result.tx_status)
    }

    fn validate_status(&self, tx_hash: &str, status: TxStatus) -> Result<VerifiedCkbTransaction> {
        let accepted = status.status == "committed"
            || (self.accept_pending && matches!(status.status.as_str(), "pending" | "proposed"));
        if accepted {
            return Ok(VerifiedCkbTransaction {
                tx_hash: tx_hash.to_string(),
                status: status.status,
            });
        }
        let reason = status
            .reason
            .filter(|reason| !reason.trim().is_empty())
            .unwrap_or_else(|| "no rejection reason provided".to_string());
        Err(anyhow!(
            "CKB transaction is not accepted for settlement: status={}, reason={}",
            status.status,
            reason
        ))
    }
}
pub fn explicit_transaction_hash(value: &Value) -> Option<&str> {
    value
        .get("hash")
        .and_then(|hash| hash.as_str())
        .map(str::trim)
        .filter(|hash| !hash.is_empty())
}

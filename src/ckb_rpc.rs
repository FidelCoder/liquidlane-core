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

#[derive(Debug, Deserialize)]
struct GetCellsResult {
    objects: Vec<CkbLiveCell>,
}

#[derive(Debug, Deserialize)]
pub struct CkbLiveCell {
    pub out_point: CkbOutPoint,
    pub output: Value,
    pub output_data: String,
}

#[derive(Debug, Deserialize)]
pub struct CkbOutPoint {
    pub tx_hash: String,
    pub index: String,
}

impl CkbOutPoint {
    pub fn cell_out_point(&self) -> String {
        format!("{}#{}", self.tx_hash, self.index)
    }
}

#[derive(Debug, Serialize)]
pub struct VerifiedCkbTransaction {
    pub tx_hash: String,
    pub status: String,
}

#[derive(Debug)]
pub struct CkbTransactionDetails {
    pub transaction: Value,
}

impl CkbRpcClient {
    pub fn new(url: String, accept_pending: bool) -> Self {
        Self {
            client: Client::new(),
            url,
            accept_pending,
        }
    }

    pub async fn live_vault_cells_by_code(
        &self,
        vault_type_code_hash: &str,
        vault_lock_code_hash: &str,
        limit: u32,
    ) -> Result<Vec<CkbLiveCell>> {
        self.get_cells(
            json!({
                "script": code_script(vault_type_code_hash, "data1", "0x"),
                "script_type": "type",
                "script_search_mode": "prefix",
                "filter": {
                    "script": code_script(vault_lock_code_hash, "data1", "0x"),
                    "script_search_mode": "prefix"
                }
            }),
            limit,
        )
        .await
    }

    pub async fn live_cells_by_type_code(
        &self,
        type_code_hash: &str,
        limit: u32,
    ) -> Result<Vec<CkbLiveCell>> {
        self.get_cells(
            json!({
                "script": code_script(type_code_hash, "data1", "0x"),
                "script_type": "type",
                "script_search_mode": "prefix"
            }),
            limit,
        )
        .await
    }

    pub async fn live_cells_by_lock_and_type_code(
        &self,
        lock_code_hash: &str,
        lock_hash_type: &str,
        lock_args: &str,
        type_code_hash: &str,
        limit: u32,
    ) -> Result<Vec<CkbLiveCell>> {
        self.get_cells(
            json!({
                "script": code_script(lock_code_hash, lock_hash_type, lock_args),
                "script_type": "lock",
                "filter": {
                    "script": code_script(type_code_hash, "data1", "0x"),
                    "script_search_mode": "prefix"
                }
            }),
            limit,
        )
        .await
    }

    async fn get_cells(&self, search_key: Value, limit: u32) -> Result<Vec<CkbLiveCell>> {
        let response = self
            .client
            .post(&self.url)
            .json(&json!({
                "id": 1,
                "jsonrpc": "2.0",
                "method": "get_cells",
                "params": [search_key, "asc", format!("0x{limit:x}")]
            }))
            .send()
            .await?
            .error_for_status()?
            .json::<RpcResponse<GetCellsResult>>()
            .await?;

        if let Some(error) = response.error {
            return Err(anyhow!(
                "CKB RPC get_cells failed: {} ({})",
                error.message,
                error.code
            ));
        }
        Ok(response
            .result
            .map(|result| result.objects)
            .unwrap_or_default())
    }

    pub async fn transaction_details(&self, tx_hash: &str) -> Result<CkbTransactionDetails> {
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
            .json::<RpcResponse<Value>>()
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
        let status = result
            .get("tx_status")
            .and_then(|status| status.get("status"))
            .and_then(|status| status.as_str())
            .ok_or_else(|| anyhow!("CKB transaction status was missing"))?
            .to_string();
        let tx_status = TxStatus {
            status: status.clone(),
            reason: result
                .get("tx_status")
                .and_then(|status| status.get("reason"))
                .and_then(|reason| reason.as_str())
                .map(str::to_string),
        };
        self.validate_status(tx_hash, tx_status)?;
        let transaction = result
            .get("transaction")
            .cloned()
            .ok_or_else(|| anyhow!("CKB transaction payload was missing"))?;
        Ok(CkbTransactionDetails { transaction })
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

fn code_script(code_hash: &str, hash_type: &str, args: &str) -> Value {
    json!({
        "code_hash": code_hash,
        "hash_type": hash_type,
        "args": args
    })
}

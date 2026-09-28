use super::Connector;
use crate::marketplace::{
    chain, crypto,
    model::{SHANNONS, TESTNET_GENESIS},
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};

impl Connector {
    pub async fn fiber(&self, method: &str, params: Value) -> Result<Value> {
        chain::rpc(&self.client, &self.config.fiber_rpc, method, params).await
    }
    pub async fn node_info(&self) -> Result<Value> {
        let node = self.fiber("node_info", json!([])).await?;
        ensure!(
            node["pubkey"] == crypto::pubkey(&self.secret),
            "Fiber RPC identity mismatch"
        );
        ensure!(
            node["chain_hash"] == TESTNET_GENESIS,
            "Fiber node is not on CKB testnet"
        );
        ensure!(
            node["version"] == self.config.fiber_version,
            "Fiber version differs from local pin"
        );
        Ok(node)
    }
    pub async fn channels(&self) -> Result<Vec<Value>> {
        let value = self
            .fiber("list_channels", json!([{"include_closed":true}]))
            .await?;
        Ok(value["channels"]
            .as_array()
            .context("channel list unavailable")?
            .clone())
    }
    pub async fn available_ckb(&self, node: &Value) -> Result<u64> {
        let script = node
            .get("default_funding_lock_script")
            .context("node funding lock missing")?;
        let mut cursor: Option<Value> = None;
        let mut total = 0u64;
        loop {
            let mut params = json!([{"script":script,"script_type":"lock","script_search_mode":"exact","filter":{"script_len_range":["0x0","0x1"],"output_data_len_range":["0x0","0x1"]}},"asc","0x64"]);
            if let Some(cursor) = &cursor {
                params.as_array_mut().expect("array").push(cursor.clone());
            }
            let result =
                chain::rpc(&self.client, &self.config.ckb_rpc, "get_cells", params).await?;
            let cells = result["objects"]
                .as_array()
                .context("CKB indexer unavailable")?;
            for cell in cells {
                if cell["output"]["type"].is_null() && cell["output_data"] == "0x" {
                    total = total
                        .checked_add(chain::number(&cell["output"]["capacity"])?)
                        .context("wallet balance overflow")?;
                }
            }
            if cells.len() < 100 {
                break;
            }
            let next = result["last_cursor"].clone();
            ensure!(
                next.is_string() && cursor.as_ref() != Some(&next),
                "CKB indexer cursor did not advance"
            );
            cursor = Some(next);
        }
        Ok(total / SHANNONS)
    }
}

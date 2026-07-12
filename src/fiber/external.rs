use anyhow::{Result, anyhow};
use serde_json::{Map, Value, json};

use super::{FiberClient, funding_amount_hex, script_to_value, string_field};
use crate::domain::{CkbScript, LiquidityRequest};

#[derive(Clone, Debug)]
pub struct FiberExternalFundingParams {
    pub funding_lock_script: CkbScript,
    pub shutdown_script: CkbScript,
    pub funding_lock_script_cell_deps: Vec<Value>,
    pub public_channel: bool,
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct FiberExternalFundingOutcome {
    pub temporary_channel_id: Option<String>,
    pub channel_id: Option<String>,
    pub note: Option<String>,
    pub raw: Value,
}

impl FiberClient {
    #[allow(dead_code)]
    pub async fn open_channel_with_external_funding(
        &self,
        request: &LiquidityRequest,
        funding: &FiberExternalFundingParams,
    ) -> Result<FiberExternalFundingOutcome> {
        let rpc_url = self.rpc_url()?;
        self.validate_external_request(request).await?;
        self.connect_peer(rpc_url, request).await?;
        let result = self
            .rpc_call(
                rpc_url,
                "open_channel_with_external_funding",
                request.id.to_string(),
                external_funding_params(request, funding)?,
            )
            .await?;
        Ok(FiberExternalFundingOutcome {
            temporary_channel_id: string_field(&result, "temporary_channel_id"),
            channel_id: string_field(&result, "channel_id"),
            note: Some(
                "Fiber external funding negotiated. Waiting for LiquidLane vault funding transaction."
                    .to_string(),
            ),
            raw: result,
        })
    }

    #[allow(dead_code)]
    pub async fn submit_signed_funding_tx(
        &self,
        channel_id: &str,
        signed_funding_tx: Value,
    ) -> Result<Value> {
        let rpc_url = self.rpc_url()?;
        if channel_id.trim().is_empty() {
            return Err(anyhow!(
                "Fiber channel id is required to submit external funding tx"
            ));
        }
        self.rpc_call(
            rpc_url,
            "submit_signed_funding_tx",
            format!("liquidlane-submit-funding-{channel_id}"),
            json!({
                "channel_id": channel_id,
                "signed_funding_tx": signed_funding_tx,
            }),
        )
        .await
    }

    #[allow(dead_code)]
    async fn validate_external_request(&self, request: &LiquidityRequest) -> Result<()> {
        let rpc_url = self.rpc_url()?;
        let peer_pubkey = request
            .fiber_peer_pubkey
            .as_deref()
            .ok_or_else(|| anyhow!("fiber_peer_pubkey is required for Fiber external funding"))?;
        if request.asset != "CKB" && request.funding_udt_type_script.is_none() {
            return Err(anyhow!(
                "funding_udt_type_script is required to externally fund a {} Fiber channel",
                request.asset
            ));
        }
        self.reject_self_peer(rpc_url, peer_pubkey).await
    }
}

pub(super) fn external_funding_params(
    request: &LiquidityRequest,
    funding: &FiberExternalFundingParams,
) -> Result<Value> {
    let peer_pubkey = request
        .fiber_peer_pubkey
        .as_deref()
        .ok_or_else(|| anyhow!("fiber_peer_pubkey is required for Fiber external funding"))?;
    let mut params = Map::new();
    params.insert("pubkey".to_string(), Value::String(peer_pubkey.to_string()));
    params.insert(
        "funding_amount".to_string(),
        Value::String(funding_amount_hex(&request.asset, request.amount)?),
    );
    params.insert("public".to_string(), json!(funding.public_channel));
    params.insert(
        "funding_lock_script".to_string(),
        script_to_value(&funding.funding_lock_script),
    );
    params.insert(
        "shutdown_script".to_string(),
        script_to_value(&funding.shutdown_script),
    );
    if !funding.funding_lock_script_cell_deps.is_empty() {
        params.insert(
            "funding_lock_script_cell_deps".to_string(),
            Value::Array(funding.funding_lock_script_cell_deps.clone()),
        );
    }
    if let Some(script) = request.funding_udt_type_script.as_ref() {
        params.insert(
            "funding_udt_type_script".to_string(),
            script_to_value(script),
        );
    }
    Ok(Value::Object(params))
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use uuid::Uuid;

    use super::*;
    use crate::domain::{CkbScript, LiquidityStatus};

    #[test]
    fn builds_external_funding_params_for_ckb() {
        let params = external_funding_params(&request(), &funding()).unwrap();
        assert_eq!(params["pubkey"], "03peer");
        assert_eq!(params["funding_amount"], "0x4a817c800");
        assert_eq!(params["public"], false);
        assert_eq!(params["funding_lock_script"]["hash_type"], "type");
        assert_eq!(params["shutdown_script"]["args"], "0xshutdown");
        assert!(params.get("funding_lock_script_cell_deps").is_none());
    }

    fn funding() -> FiberExternalFundingParams {
        FiberExternalFundingParams {
            funding_lock_script: script("0xfunding"),
            shutdown_script: script("0xshutdown"),
            funding_lock_script_cell_deps: Vec::new(),
            public_channel: false,
        }
    }

    fn script(args: &str) -> CkbScript {
        CkbScript {
            code_hash: "0x1111111111111111111111111111111111111111111111111111111111111111"
                .to_string(),
            hash_type: "type".to_string(),
            args: args.to_string(),
        }
    }

    fn request() -> LiquidityRequest {
        let now = Utc::now();
        LiquidityRequest {
            id: Uuid::new_v4(),
            merchant_id: Uuid::new_v4(),
            merchant_name: "Merchant".to_string(),
            ckb_address: "ckt1qmerchant".to_string(),
            asset: "CKB".to_string(),
            amount: 200,
            duration_days: 1,
            lease_fee: 1,
            routing_fee_bps: 30,
            fiber_peer_pubkey: Some("03peer".to_string()),
            fiber_peer_address: None,
            public_channel: false,
            funding_udt_type_script: None,
            request_cell_id: "ll-request-test".to_string(),
            request_tx_hash: Some("0xrequest".to_string()),
            request_cell_out_point: Some("0xrequest#0x0".to_string()),
            status: LiquidityStatus::Requested,
            fiber_temporary_channel_id: None,
            channel_id: None,
            fiber_note: None,
            fiber_error: None,
            created_at: now,
            updated_at: now,
        }
    }
}

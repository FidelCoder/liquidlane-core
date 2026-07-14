use anyhow::{Result, anyhow};
use chrono::Utc;
use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;

use super::AppStore;
use crate::domain::{
    ActivityEvent, ExternalFundingIntentStatus, LiquidityRequest, LiquidityStatus, VaultConfig,
};

const SHANNONS_PER_CKB: u128 = 100_000_000;

#[derive(Clone, Debug, Deserialize)]
pub(super) struct FiberFundingBuilderPayload {
    pub tx: ckb_jsonrpc_types::Transaction,
    pub request: FiberFundingRequest,
    pub rpc_url: String,
    pub funding_source_lock_script: String,
}

#[derive(Clone, Debug, Deserialize)]
pub(super) struct FiberFundingRequest {
    pub script: String,
    #[serde(default)]
    pub udt_type_script: Option<String>,
    pub local_amount: u128,
    pub funding_fee_rate: u64,
    #[serde(default)]
    pub remote_amount: u128,
    #[serde(default)]
    pub local_reserved_ckb_amount: u64,
    #[serde(default)]
    pub remote_reserved_ckb_amount: u64,
}

#[derive(Clone, Debug)]
pub(super) struct MatchedFundingRequest {
    pub vault: VaultConfig,
    pub request: LiquidityRequest,
}

#[derive(Clone, Debug)]
pub(super) struct BuiltFiberFundingTx {
    pub transaction: Value,
    pub tx_hash: String,
    pub funding_out_point: String,
    pub request_id: Uuid,
}

impl AppStore {
    pub async fn build_fiber_funding_transaction(&self, payload: Value) -> Result<Value> {
        let payload: FiberFundingBuilderPayload = serde_json::from_value(payload)
            .map_err(|err| anyhow!("invalid Fiber funding builder payload: {err}"))?;
        tracing::info!(
            local_amount = payload.request.local_amount,
            remote_amount = payload.request.remote_amount,
            funding_fee_rate = payload.request.funding_fee_rate,
            "Fiber funding builder callback received"
        );
        let matched = self.match_fiber_funding_request(&payload).await?;
        tracing::info!(
            request_id = %matched.request.id,
            amount = matched.request.amount,
            merchant = %matched.request.merchant_name,
            "Fiber funding builder matched LiquidLane reserve"
        );
        let built = tokio::task::spawn_blocking(move || {
            super::fiber_funding_tx::build_vault_funding_transaction(matched, payload)
        })
        .await
        .map_err(|err| anyhow!("vault funding builder task failed: {err}"))??;
        self.mark_fiber_funding_built(&built).await?;
        Ok(built.transaction)
    }

    async fn match_fiber_funding_request(
        &self,
        payload: &FiberFundingBuilderPayload,
    ) -> Result<MatchedFundingRequest> {
        if payload.request.udt_type_script.is_some() {
            return Err(anyhow!(
                "LiquidLane vault-funded beta supports CKB funding only"
            ));
        }
        if payload.request.remote_amount != 0 {
            return Err(anyhow!(
                "LiquidLane vault-funded beta expects merchant-side receive capacity without remote channel balance"
            ));
        }
        if payload.request.funding_fee_rate == 0 {
            return Err(anyhow!("Fiber funding fee rate must be positive"));
        }
        let funded_shannons = payload
            .request
            .local_amount
            .checked_add(u128::from(payload.request.local_reserved_ckb_amount))
            .ok_or_else(|| anyhow!("Fiber funding amount exceeds LiquidLane u128 range"))?;
        if funded_shannons % SHANNONS_PER_CKB != 0 {
            return Err(anyhow!(
                "Fiber local funding amount plus reserve must be a whole CKB amount"
            ));
        }
        let amount = u64::try_from(funded_shannons / SHANNONS_PER_CKB)
            .map_err(|_| anyhow!("Fiber funding amount exceeds LiquidLane u64 range"))?;
        let state = self.inner.read().await;
        let mut matches = state
            .liquidity_requests
            .iter()
            .filter(|request| request.amount == amount)
            .filter(|request| request.asset == "CKB")
            .filter(|request| request.request_cell_out_point.is_some())
            .filter(|request| request.fiber_peer_pubkey.is_some())
            .filter(|request| {
                matches!(
                    request.status,
                    LiquidityStatus::FundingRequired
                        | LiquidityStatus::FundingSubmitted
                        | LiquidityStatus::PendingFiberChannel
                )
            })
            .filter(|request| {
                state
                    .external_funding_intents
                    .iter()
                    .find(|intent| intent.request_id == request.id)
                    .is_none_or(|intent| {
                        intent.funding_tx_hash.is_none() && intent.funding_out_point.is_none()
                    })
            })
            .cloned()
            .collect::<Vec<_>>();
        matches.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        let candidate_count = state.liquidity_requests.len();
        let request = matches.into_iter().next().ok_or_else(|| {
            tracing::warn!(
                amount,
                candidates = candidate_count,
                "Fiber funding builder could not match callback to an in-flight reserve"
            );
            anyhow!(
                "no in-flight LiquidLane vault reserve matches Fiber funding amount {amount} CKB"
            )
        })?;
        drop(state);
        let vault = self.vault_config().await;
        Ok(MatchedFundingRequest { vault, request })
    }

    async fn mark_fiber_funding_built(&self, built: &BuiltFiberFundingTx) -> Result<()> {
        let now = Utc::now();
        let mut state = self.inner.write().await;
        let Some(request) = state
            .liquidity_requests
            .iter_mut()
            .find(|request| request.id == built.request_id)
        else {
            self.persist_locked(&state).await?;
            return Ok(());
        };
        tracing::info!(
            request_id = %request.id,
            tx_hash = %built.tx_hash,
            funding_out_point = %built.funding_out_point,
            "vault-funded CKB funding transaction built"
        );
        request.status = LiquidityStatus::FundingSubmitted;
        request.fiber_temporary_channel_id = Some(built.tx_hash.clone());
        request.fiber_note = Some(
            "Vault-funded CKB transaction was built for Fiber signing and broadcast.".to_string(),
        );
        request.fiber_error = None;
        request.updated_at = now;
        let request_id = request.id;
        let merchant_id = request.merchant_id;
        let amount = request.amount;
        let asset = request.asset.clone();
        let merchant_name = request.merchant_name.clone();
        if let Some(intent) = state
            .external_funding_intents
            .iter_mut()
            .find(|intent| intent.request_id == request_id)
        {
            intent.status = ExternalFundingIntentStatus::FundingSubmitted;
            intent.funding_tx_hash = Some(built.tx_hash.clone());
            intent.funding_out_point = Some(built.funding_out_point.clone());
            intent.fiber_ref = Some(built.tx_hash.clone());
            intent.note =
                "Vault-funded CKB transaction built; Fiber is signing and broadcasting it."
                    .to_string();
            intent.blockers.clear();
            intent.updated_at = now;
        }
        state.events.insert(
            0,
            ActivityEvent {
                id: Uuid::new_v4(),
                actor_id: merchant_id,
                label: format!("Vault-funded Fiber tx built for {merchant_name}"),
                amount: Some(amount),
                asset: Some(asset),
                created_at: now,
            },
        );
        self.persist_locked(&state).await
    }
}

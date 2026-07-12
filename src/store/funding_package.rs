use anyhow::{Result, anyhow};
use chrono::Utc;
use uuid::Uuid;

use super::{AppStore, liquidity_deploy::update_reservation_and_positions};
use crate::domain::{
    ActivityEvent, ExternalFundingIntent, ExternalFundingIntentStatus, ExternalFundingPlan,
    ExternalFundingSubmitRequest, ExternalFundingSubmitResponse, LiquidityRequest, LiquidityStatus,
};

impl AppStore {
    pub async fn external_funding_plan(&self, id: Uuid) -> Result<ExternalFundingPlan> {
        let request = self.stored_liquidity_request(id).await?;
        let preview = self.external_funding_preview(id).await?;
        let readiness = self.external_funding_readiness().await;
        let vault = self.vault_config().await;
        let mut blockers = preview.blockers;
        if !matches!(request.status, LiquidityStatus::FundingRequired) {
            blockers.push(
                "request must be funding_required before a vault funding tx can be built"
                    .to_string(),
            );
        }
        let ready_for_signing = blockers.is_empty() && readiness.funding_signer_ready;
        let ready_for_submission = ready_for_signing && self.ckb_rpc.is_some();
        Ok(ExternalFundingPlan {
            request_id: request.id,
            amount: request.amount,
            asset: request.asset.clone(),
            vault_cell_out_point: vault.cell_out_point,
            request_cell_out_point: request.request_cell_out_point.clone(),
            funding_lock_target: funding_target(&request),
            required_signer: "liquidlane_vault_funding_authority".to_string(),
            unsigned_tx_available: readiness.funding_tx_builder_ready && blockers.is_empty(),
            ready_for_signing,
            ready_for_submission,
            next_action: funding_next_action(&blockers, ready_for_submission),
            blockers,
        })
    }

    pub async fn submit_external_funding_tx(
        &self,
        id: Uuid,
        request: ExternalFundingSubmitRequest,
    ) -> Result<ExternalFundingSubmitResponse> {
        let plan = self.external_funding_plan(id).await?;
        if !plan.ready_for_submission {
            return Err(anyhow!(plan.next_action));
        }
        self.verify_ckb_settlement_tx(&request.tx_hash, &request.signed_tx)
            .await?;
        let now = Utc::now();
        let mut state = self.inner.write().await;
        let updated = mark_request_funding_submitted(&mut state, id, &request, now)?;
        let intent = mark_intent_funding_submitted(&mut state, &updated, &request, now);
        update_reservation_and_positions(&mut state, &updated, now);
        state.events.insert(
            0,
            ActivityEvent {
                id: Uuid::new_v4(),
                actor_id: updated.merchant_id,
                label: format!(
                    "Vault-funded Fiber tx submitted for {}",
                    updated.merchant_name
                ),
                amount: Some(updated.amount),
                asset: Some(updated.asset.clone()),
                created_at: now,
            },
        );
        self.persist_locked(&state).await?;
        Ok(ExternalFundingSubmitResponse {
            request: updated,
            intent,
        })
    }
}

fn mark_request_funding_submitted(
    state: &mut super::StoreState,
    id: Uuid,
    request: &ExternalFundingSubmitRequest,
    now: chrono::DateTime<Utc>,
) -> Result<LiquidityRequest> {
    let stored = state
        .liquidity_requests
        .iter_mut()
        .find(|stored| stored.id == id)
        .ok_or_else(|| anyhow!("liquidity request not found"))?;
    stored.status = LiquidityStatus::FundingSubmitted;
    stored.fiber_error = None;
    stored.fiber_note = Some(
        "Vault-funded CKB transaction confirmed; waiting for Fiber channel activation.".to_string(),
    );
    stored.fiber_temporary_channel_id = Some(request.tx_hash.clone());
    stored.updated_at = now;
    Ok(stored.clone())
}

fn mark_intent_funding_submitted(
    state: &mut super::StoreState,
    request: &LiquidityRequest,
    funding: &ExternalFundingSubmitRequest,
    now: chrono::DateTime<Utc>,
) -> ExternalFundingIntent {
    if let Some(intent) = state
        .external_funding_intents
        .iter_mut()
        .find(|intent| intent.request_id == request.id)
    {
        intent.status = ExternalFundingIntentStatus::FundingSubmitted;
        intent.blockers.clear();
        intent.funding_tx_hash = Some(funding.tx_hash.clone());
        intent.funding_out_point = funding.funding_out_point.clone();
        intent.note = "Vault funding transaction confirmed; waiting for Fiber channel activation."
            .to_string();
        intent.updated_at = now;
        return intent.clone();
    }
    let intent = ExternalFundingIntent {
        id: Uuid::new_v4(),
        request_id: request.id,
        merchant_id: request.merchant_id,
        merchant_name: request.merchant_name.clone(),
        ckb_address: request.ckb_address.clone(),
        asset: request.asset.clone(),
        amount: request.amount,
        request_tx_hash: request.request_tx_hash.clone(),
        request_cell_out_point: request.request_cell_out_point.clone(),
        fiber_peer_pubkey: request.fiber_peer_pubkey.clone(),
        fiber_peer_address: request.fiber_peer_address.clone(),
        status: ExternalFundingIntentStatus::FundingSubmitted,
        blockers: Vec::new(),
        funding_tx_hash: Some(funding.tx_hash.clone()),
        funding_out_point: funding.funding_out_point.clone(),
        fiber_ref: request.fiber_temporary_channel_id.clone(),
        note: "Vault funding transaction confirmed; waiting for Fiber channel activation."
            .to_string(),
        created_at: now,
        updated_at: now,
    };
    state.external_funding_intents.push(intent.clone());
    intent
}

fn funding_target(request: &LiquidityRequest) -> Option<String> {
    request
        .fiber_temporary_channel_id
        .clone()
        .or_else(|| request.channel_id.clone())
}

fn funding_next_action(blockers: &[String], ready_for_submission: bool) -> String {
    if ready_for_submission {
        return "Submit a verified vault-funded CKB transaction hash for Fiber finalization."
            .to_string();
    }
    blockers.first().cloned().unwrap_or_else(|| {
        "CKB RPC and funding signer readiness are required before submission.".to_string()
    })
}

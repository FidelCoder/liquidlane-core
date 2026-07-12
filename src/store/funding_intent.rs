use anyhow::Result;
use chrono::Utc;
use uuid::Uuid;

use super::AppStore;
use crate::domain::{
    ActivityEvent, ExternalFundingIntent, ExternalFundingIntentStatus, ExternalFundingReadiness,
    LiquidityRequest, LiquidityStatus, is_vault_external_funding_mode,
};

impl AppStore {
    pub async fn external_funding_readiness(&self) -> ExternalFundingReadiness {
        let vault = self.vault_config().await;
        let supported = is_vault_external_funding_mode(&self.executor_funding_mode);
        let vault_configured = vault.configured;
        let fiber_rpc_configured = self.fiber.is_configured();
        let v2_scripts_configured = false;
        let funding_tx_builder_ready = false;
        let mut blockers = Vec::new();

        if !supported {
            blockers.push(
                "LiquidLane is running node-wallet diagnostic mode, not vault-funded product mode."
                    .to_string(),
            );
        }
        if !vault_configured {
            blockers.push("LiquidLane vault cell is not configured.".to_string());
        }
        if !fiber_rpc_configured {
            blockers.push("Fiber RPC is not configured.".to_string());
        }
        if !v2_scripts_configured {
            blockers
                .push("Vault v2 external-funding scripts are not deployed/configured.".to_string());
        }
        if !funding_tx_builder_ready {
            blockers
                .push("Vault-funded CKB funding transaction builder is not enabled.".to_string());
        }

        ExternalFundingReadiness {
            supported,
            ready: supported
                && vault_configured
                && fiber_rpc_configured
                && v2_scripts_configured
                && funding_tx_builder_ready,
            funding_mode: self.executor_funding_mode.clone(),
            vault_configured,
            fiber_rpc_configured,
            v2_scripts_configured,
            funding_tx_builder_ready,
            blockers,
        }
    }

    pub async fn external_funding_intents(&self) -> Vec<ExternalFundingIntent> {
        let mut intents = self.inner.read().await.external_funding_intents.clone();
        intents.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        intents
    }

    pub(super) async fn prepare_external_funding_intent(
        &self,
        request: &LiquidityRequest,
        actor_id: Uuid,
        executor: bool,
    ) -> Result<LiquidityRequest> {
        let readiness = self.external_funding_readiness().await;
        let blockers = readiness.blockers.clone();
        let now = Utc::now();
        let mut state = self.inner.write().await;
        let intent_id = if let Some(intent) = state
            .external_funding_intents
            .iter_mut()
            .find(|intent| intent.request_id == request.id)
        {
            intent.status = ExternalFundingIntentStatus::BuilderRequired;
            intent.blockers = blockers.clone();
            intent.note = external_funding_note(&blockers);
            intent.fiber_peer_pubkey = request.fiber_peer_pubkey.clone();
            intent.fiber_peer_address = request.fiber_peer_address.clone();
            intent.updated_at = now;
            intent.id
        } else {
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
                status: ExternalFundingIntentStatus::BuilderRequired,
                blockers: blockers.clone(),
                funding_tx_hash: None,
                fiber_ref: None,
                note: external_funding_note(&blockers),
                created_at: now,
                updated_at: now,
            };
            let id = intent.id;
            state.external_funding_intents.push(intent);
            id
        };

        let stored = state
            .liquidity_requests
            .iter_mut()
            .find(|stored| stored.id == request.id)
            .ok_or_else(|| anyhow::anyhow!("liquidity request not found"))?;
        stored.status = LiquidityStatus::FundingRequired;
        stored.fiber_temporary_channel_id = Some(intent_id.to_string());
        stored.fiber_note = Some(external_funding_note(&blockers));
        stored.fiber_error = blockers.first().cloned();
        stored.updated_at = now;
        let updated = stored.clone();

        state.events.insert(
            0,
            ActivityEvent {
                id: Uuid::new_v4(),
                actor_id: if executor {
                    updated.merchant_id
                } else {
                    actor_id
                },
                label: format!(
                    "Vault-funded Fiber transaction required for {}",
                    updated.merchant_name
                ),
                amount: Some(updated.amount),
                asset: Some(updated.asset.clone()),
                created_at: now,
            },
        );
        self.persist_locked(&state).await?;
        Ok(updated)
    }
}

fn external_funding_note(blockers: &[String]) -> String {
    if blockers.is_empty() {
        return "Vault reserve is confirmed. LiquidLane is preparing the Fiber funding transaction from LP vault liquidity.".to_string();
    }
    format!(
        "Vault reserve is confirmed, but Fiber funding needs the v2 vault-funded transaction path: {}",
        blockers.join(" ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::LiquidityStatus;

    #[tokio::test]
    async fn prepares_repairable_external_funding_intent() {
        let store = AppStore::memory();
        let request = request();
        {
            let mut state = store.inner.write().await;
            state.liquidity_requests.push(request.clone());
        }

        let updated = store
            .prepare_external_funding_intent(&request, request.merchant_id, true)
            .await
            .unwrap();

        assert_eq!(updated.status, LiquidityStatus::FundingRequired);
        assert!(updated.fiber_error.is_some());
        let intents = store.external_funding_intents().await;
        assert_eq!(intents.len(), 1);
        assert_eq!(intents[0].request_id, request.id);
        assert_eq!(intents[0].amount, request.amount);
        assert_eq!(
            intents[0].status,
            ExternalFundingIntentStatus::BuilderRequired
        );
    }

    #[tokio::test]
    async fn readiness_reports_builder_blocker() {
        let readiness = AppStore::memory().external_funding_readiness().await;
        assert!(!readiness.ready);
        assert!(
            readiness
                .blockers
                .iter()
                .any(|blocker| blocker.contains("transaction builder"))
        );
    }

    fn request() -> LiquidityRequest {
        let now = Utc::now();
        LiquidityRequest {
            id: Uuid::new_v4(),
            merchant_id: Uuid::new_v4(),
            merchant_name: "merchant".to_string(),
            ckb_address: "ckt1test".to_string(),
            asset: "CKB".to_string(),
            amount: 200,
            duration_days: 30,
            lease_fee: 1,
            routing_fee_bps: 30,
            fiber_peer_pubkey: Some(
                "02b6d4e3ab86a2ca2fad6fae0ecb2e1e559e0b911939872a90abdda6d20302be71".to_string(),
            ),
            fiber_peer_address: None,
            public_channel: false,
            funding_udt_type_script: None,
            request_cell_id: "ll-request-test".to_string(),
            request_tx_hash: Some(
                "0x1111111111111111111111111111111111111111111111111111111111111111".to_string(),
            ),
            request_cell_out_point: Some(
                "0x1111111111111111111111111111111111111111111111111111111111111111#0x0"
                    .to_string(),
            ),
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

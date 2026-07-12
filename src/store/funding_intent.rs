use anyhow::Result;
use chrono::Utc;
use uuid::Uuid;

use super::AppStore;
use crate::domain::{
    ActivityEvent, ExternalFundingIntent, ExternalFundingIntentStatus, ExternalFundingPreview,
    ExternalFundingReadiness, LiquidityRequest, LiquidityStatus, VaultConfig,
    is_vault_external_funding_mode,
};

impl AppStore {
    pub async fn external_funding_readiness(&self) -> ExternalFundingReadiness {
        let vault = self.vault_config().await;
        let supported = is_vault_external_funding_mode(&self.executor_funding_mode);
        let vault_configured = vault.configured;
        let fiber_rpc_configured = self.fiber.is_configured();
        let v2_scripts_configured = v2_scripts_configured(&vault);
        let funding_tx_builder_ready = self.vault_funding_builder_enabled;
        let funding_signer_ready = self.vault_funding_signer_enabled;
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
            blockers.push(v2_script_blocker(&vault));
        }
        if !funding_tx_builder_ready {
            blockers
                .push("Vault-funded CKB funding transaction builder is not enabled.".to_string());
        }
        if !funding_signer_ready {
            blockers
                .push("Vault funding signer is not enabled for testnet submission.".to_string());
        }

        let ready = supported
            && vault_configured
            && fiber_rpc_configured
            && v2_scripts_configured
            && funding_tx_builder_ready
            && funding_signer_ready;
        let next_action = if ready {
            "Negotiate Fiber external funding and build the CKB funding transaction.".to_string()
        } else {
            blockers
                .first()
                .cloned()
                .unwrap_or_else(|| "External funding readiness is incomplete.".to_string())
        };

        ExternalFundingReadiness {
            supported,
            ready,
            funding_mode: self.executor_funding_mode.clone(),
            vault_configured,
            fiber_rpc_configured,
            v2_scripts_configured,
            funding_tx_builder_ready,
            funding_signer_ready,
            blockers,
            next_action,
        }
    }

    pub async fn external_funding_intents(&self) -> Vec<ExternalFundingIntent> {
        let mut intents = self.inner.read().await.external_funding_intents.clone();
        intents.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        intents
    }

    pub async fn external_funding_preview(&self, id: Uuid) -> Result<ExternalFundingPreview> {
        let request = self.stored_liquidity_request(id).await?;
        let readiness = self.external_funding_readiness().await;
        let mut blockers = readiness.blockers.clone();
        if request.request_tx_hash.is_none() {
            blockers.push("Capacity request transaction is not confirmed yet.".to_string());
        }
        if request
            .fiber_peer_pubkey
            .as_deref()
            .unwrap_or_default()
            .is_empty()
        {
            blockers.push("Merchant Fiber receive pubkey is missing.".to_string());
        }
        let ready = blockers.is_empty();
        Ok(ExternalFundingPreview {
            request_id: request.id,
            amount: request.amount,
            asset: request.asset,
            fiber_peer_pubkey: request.fiber_peer_pubkey,
            request_tx_hash: request.request_tx_hash,
            request_cell_out_point: request.request_cell_out_point,
            ready,
            next_action: if ready {
                "Build and dry-run the vault-funded CKB funding transaction.".to_string()
            } else {
                blockers
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "External funding cannot start yet.".to_string())
            },
            blockers,
        })
    }

    pub async fn retry_external_funding_request(&self, id: Uuid) -> Result<LiquidityRequest> {
        let request = self.stored_liquidity_request(id).await?;
        if matches!(
            request.status,
            LiquidityStatus::ChannelOpen | LiquidityStatus::Released | LiquidityStatus::Expired
        ) {
            anyhow::bail!("this request is not retryable");
        }
        self.submit_fiber_handoff(id, Uuid::nil(), true).await
    }

    pub async fn external_funding_stuck_requests(&self) -> Vec<LiquidityRequest> {
        let state = self.inner.read().await;
        state
            .liquidity_requests
            .iter()
            .filter(|request| {
                matches!(
                    request.status,
                    LiquidityStatus::FundingRequired
                        | LiquidityStatus::FundingSubmitted
                        | LiquidityStatus::PendingFiberChannel
                )
            })
            .filter(|request| request.fiber_error.is_some() || request.fiber_note.is_some())
            .cloned()
            .collect()
    }

    pub async fn external_funding_release_candidates(&self) -> Vec<LiquidityRequest> {
        let now = Utc::now();
        let state = self.inner.read().await;
        state
            .liquidity_requests
            .iter()
            .filter(|request| {
                matches!(
                    request.status,
                    LiquidityStatus::FundingRequired | LiquidityStatus::Failed
                ) && request.created_at + chrono::Duration::days(i64::from(request.duration_days))
                    <= now
            })
            .cloned()
            .collect()
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
                funding_out_point: None,
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

fn v2_scripts_configured(vault: &VaultConfig) -> bool {
    let scripts = &vault.scripts;
    vault.script_version == "v2"
        && present(&scripts.vault_type_code_hash)
        && present(&scripts.vault_type_out_point)
        && present(&scripts.lp_receipt_type_code_hash)
        && present(&scripts.lp_receipt_type_out_point)
        && present(&scripts.request_type_code_hash)
        && present(&scripts.request_type_out_point)
        && present(&scripts.funding_intent_type_code_hash)
        && present(&scripts.funding_intent_type_out_point)
}

fn v2_script_blocker(vault: &VaultConfig) -> String {
    if vault.script_version != "v2" {
        return "Active vault is still configured as v1; deploy or migrate to a v2 vault for Fiber funding.".to_string();
    }
    "Vault v2 external-funding script hashes/out-points are incomplete.".to_string()
}

fn present(value: &Option<String>) -> bool {
    value
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_some()
}

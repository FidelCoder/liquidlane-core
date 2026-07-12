mod accounting;
mod auth;
mod chain_address;
mod chain_deposit;
mod chain_fee_claim;
mod chain_fee_guard;
mod chain_request;
mod chain_settlement;
mod chain_types;
mod dashboard;
mod executor;
mod liquidity;
mod liquidity_deploy;
#[cfg(test)]
mod liquidity_deploy_tests;
mod liquidity_peer;
mod monitoring;
mod receipt_discovery;
mod request_discovery;
mod request_intent;
mod request_recovery;
mod settlement;
mod tx_v2;
mod validation;
mod vault;
mod vault_chain_sync;
mod vault_discovery;
mod vault_v2_codec;

use std::path::PathBuf;

pub use executor::ExecutorHealth;
pub use monitoring::CoreStateExport;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
#[cfg(test)]
use uuid::Uuid;
use vault_chain_sync::LiveVaultAccounting;

use crate::{
    ckb_rpc::{CkbRpcClient, explicit_transaction_hash},
    domain::{
        ActivityEvent, AuthChallenge, CapacityReservation, Deposit, ExecutorJob, FeeClaim,
        LiquidityRequest, LpPosition, RequestIntent, SupplyIntent, User, VaultConfig,
        WithdrawalIntent,
    },
    fiber::FiberClient,
};

pub struct AppStore {
    path: PathBuf,
    fiber: FiberClient,
    vault: VaultConfig,
    ckb_rpc: Option<CkbRpcClient>,
    require_ckb_rpc: bool,
    executor_enabled: bool,
    executor_poll_interval_ms: u64,
    executor_max_retries: u8,
    executor_funding_mode: String,
    inner: RwLock<StoreState>,
}

#[derive(Default, Serialize, Deserialize)]
struct StoreState {
    users: Vec<User>,
    challenges: Vec<AuthChallenge>,
    deposits: Vec<Deposit>,
    #[serde(default)]
    supply_intents: Vec<SupplyIntent>,
    #[serde(default)]
    lp_positions: Vec<LpPosition>,
    #[serde(default)]
    withdrawal_intents: Vec<WithdrawalIntent>,
    #[serde(default)]
    fee_claims: Vec<FeeClaim>,
    #[serde(default)]
    capacity_reservations: Vec<CapacityReservation>,
    #[serde(default)]
    request_intents: Vec<RequestIntent>,
    liquidity_requests: Vec<LiquidityRequest>,
    #[serde(default)]
    executor_jobs: Vec<ExecutorJob>,
    events: Vec<ActivityEvent>,
    #[serde(default)]
    vault_address: Option<String>,
    #[serde(default)]
    vault_cell_out_point: Option<String>,
    #[serde(default)]
    live_vault_accounting: Option<LiveVaultAccounting>,
}

impl AppStore {
    pub async fn load(
        path: PathBuf,
        fiber: FiberClient,
        vault: VaultConfig,
        ckb_rpc: Option<CkbRpcClient>,
        require_ckb_rpc: bool,
        executor_enabled: bool,
        executor_poll_interval_ms: u64,
        executor_max_retries: u8,
        executor_funding_mode: String,
    ) -> Result<Self> {
        let state = match tokio::fs::read_to_string(&path).await {
            Ok(contents) => serde_json::from_str(&contents)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => StoreState::default(),
            Err(error) => return Err(error.into()),
        };

        Ok(Self {
            path,
            fiber,
            vault,
            ckb_rpc,
            require_ckb_rpc,
            executor_enabled,
            executor_poll_interval_ms,
            executor_max_retries,
            executor_funding_mode,
            inner: RwLock::new(state),
        })
    }

    #[cfg(test)]
    pub fn memory() -> Self {
        Self {
            path: std::env::temp_dir().join(format!("liquidlane-test-{}.json", Uuid::new_v4())),
            fiber: FiberClient::disabled(),
            ckb_rpc: None,
            require_ckb_rpc: false,
            executor_enabled: false,
            executor_poll_interval_ms: 5_000,
            executor_max_retries: 3,
            executor_funding_mode: "managed_node_beta".to_string(),
            vault: VaultConfig {
                asset: "CKB".to_string(),
                address: Some("ckt1qpkp7qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqq".to_string()),
                cell_out_point: Some(
                    "0x0000000000000000000000000000000000000000000000000000000000000000#0x0"
                        .to_string(),
                ),
                network: "testnet".to_string(),
                configured: true,
                scripts: crate::domain::VaultScripts {
                    vault_lock_code_hash: None,
                    vault_lock_out_point: None,
                    vault_type_code_hash: None,
                    vault_type_out_point: None,
                    lp_receipt_type_code_hash: None,
                    lp_receipt_type_out_point: None,
                    request_type_code_hash: None,
                    request_type_out_point: None,
                    fee_claim_type_code_hash: None,
                    fee_claim_type_out_point: None,
                },
            },
            inner: RwLock::new(StoreState::default()),
        }
    }

    pub async fn vault_config(&self) -> VaultConfig {
        let cached = {
            let state = self.inner.read().await;
            state.vault_config(&self.vault)
        };
        match self.discover_live_vault_config(&cached).await {
            Ok(Some(vault)) => vault,
            Ok(None) => cached,
            Err(error) => {
                tracing::warn!(error = %error, "failed to discover live LiquidLane vault cell");
                cached
            }
        }
    }

    async fn verify_ckb_settlement_tx(
        &self,
        tx_hash: &str,
        signed_tx: &Option<serde_json::Value>,
    ) -> Result<()> {
        if let Some(signed_tx) = signed_tx.as_ref()
            && let Some(hash) = explicit_transaction_hash(signed_tx)
            && hash != tx_hash
        {
            anyhow::bail!("signed_tx.hash must match tx_hash");
        }
        if let Some(client) = self.ckb_rpc.as_ref() {
            client.verify_transaction(tx_hash).await?;
            return Ok(());
        }
        if self.require_ckb_rpc {
            anyhow::bail!(
                "LIQUIDLANE_CKB_RPC_URL is required before accepting real CKB settlements"
            );
        }
        Ok(())
    }

    async fn persist_locked(&self, state: &StoreState) -> Result<()> {
        if let Some(parent) = self
            .path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(&self.path, serde_json::to_string_pretty(state)?).await?;
        Ok(())
    }
}

impl StoreState {
    fn vault_config(&self, base: &VaultConfig) -> VaultConfig {
        let mut vault = base.clone();
        if let Some(address) = self.vault_address.as_ref() {
            vault.address = Some(address.clone());
        }
        if let Some(out_point) = self.vault_cell_out_point.as_ref() {
            vault.cell_out_point = Some(out_point.clone());
        }
        vault.configured = vault.address.is_some() && vault.cell_out_point.is_some();
        vault
    }
}

pub(super) fn vault_output_out_point(tx_hash: &str) -> String {
    format!("{tx_hash}#0x0")
}

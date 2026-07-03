mod accounting;
mod auth;
mod dashboard;
mod liquidity;
mod settlement;
mod validation;
mod vault;

use std::path::PathBuf;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
#[cfg(test)]
use uuid::Uuid;

use crate::{
    domain::{
        ActivityEvent, AuthChallenge, CapacityReservation, Deposit, FeeClaim, LiquidityRequest,
        LpPosition, SupplyIntent, User, VaultConfig, WithdrawalIntent,
    },
    fiber::FiberClient,
};

pub struct AppStore {
    path: PathBuf,
    fiber: FiberClient,
    vault: VaultConfig,
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
    liquidity_requests: Vec<LiquidityRequest>,
    events: Vec<ActivityEvent>,
}

impl AppStore {
    pub async fn load(path: PathBuf, fiber: FiberClient, vault: VaultConfig) -> Result<Self> {
        let state = match tokio::fs::read_to_string(&path).await {
            Ok(contents) => serde_json::from_str(&contents)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => StoreState::default(),
            Err(error) => return Err(error.into()),
        };

        Ok(Self {
            path,
            fiber,
            vault,
            inner: RwLock::new(state),
        })
    }

    #[cfg(test)]
    pub fn memory() -> Self {
        Self {
            path: std::env::temp_dir().join(format!("liquidlane-test-{}.json", Uuid::new_v4())),
            fiber: FiberClient::disabled(),
            vault: VaultConfig {
                asset: "CKB".to_string(),
                address: Some("ckt1qpkp7liquidlanevault000000000000000000000000000".to_string()),
                network: "testnet".to_string(),
                configured: true,
                scripts: crate::domain::VaultScripts {
                    vault_lock_code_hash: None,
                    vault_type_code_hash: None,
                    lp_receipt_type_code_hash: None,
                    request_type_code_hash: None,
                    fee_claim_type_code_hash: None,
                },
            },
            inner: RwLock::new(StoreState::default()),
        }
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

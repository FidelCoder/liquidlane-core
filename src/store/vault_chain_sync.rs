use anyhow::{Result, anyhow};
use chrono::Utc;
use serde::{Deserialize, Serialize};

use super::{
    AppStore, StoreState,
    chain_types::{parse_vault_data, required_hash},
};
use crate::domain::{PositionStatus, VaultConfig};

const VAULT_COUNTER_SYNC_LIMIT: u32 = 10;

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct LiveVaultAccounting {
    pub asset: String,
    pub total_deposits: u64,
    pub reserved_liquidity: u64,
    pub deployed_liquidity: u64,
    pub available_liquidity: u64,
    pub fee_balance: u64,
}

impl AppStore {
    pub(super) async fn sync_live_vault_accounting(
        &self,
        vault: &VaultConfig,
        asset: &str,
    ) -> Result<Option<LiveVaultAccounting>> {
        let Some(live) = self.read_live_vault_accounting(vault, asset).await? else {
            return Ok(None);
        };
        let mut state = self.inner.write().await;
        if state.apply_live_vault_accounting(live.clone()) {
            self.persist_locked(&state).await?;
        }
        Ok(Some(live))
    }

    async fn read_live_vault_accounting(
        &self,
        vault: &VaultConfig,
        asset: &str,
    ) -> Result<Option<LiveVaultAccounting>> {
        let Some(client) = self.ckb_rpc.as_ref() else {
            return Ok(None);
        };
        let vault_lock_code = required_hash(
            vault.scripts.vault_lock_code_hash.as_deref(),
            "LIQUIDLANE_VAULT_LOCK_CODE_HASH",
        )?;
        let vault_type_code = required_hash(
            vault.scripts.vault_type_code_hash.as_deref(),
            "LIQUIDLANE_VAULT_TYPE_CODE_HASH",
        )?;
        let configured_out_point = vault.cell_out_point.as_deref();
        let cells = client
            .live_vault_cells_by_code(&vault_type_code, &vault_lock_code, VAULT_COUNTER_SYNC_LIMIT)
            .await?;
        let cell = select_vault_cell(cells, configured_out_point)?;
        let Some(cell) = cell else {
            return Ok(None);
        };
        let data = hex_bytes(&cell.output_data)?;
        let data = parse_vault_data(&data)?;
        let used = data.reserved.saturating_add(data.deployed);
        Ok(Some(LiveVaultAccounting {
            asset: asset.to_string(),
            total_deposits: data.total,
            reserved_liquidity: data.reserved,
            deployed_liquidity: data.deployed,
            available_liquidity: data.total.saturating_sub(used),
            fee_balance: data.fee_balance,
        }))
    }
}

impl StoreState {
    pub(super) fn apply_live_vault_accounting(&mut self, live: LiveVaultAccounting) -> bool {
        let mut changed = self.live_vault_accounting.as_ref() != Some(&live);
        self.live_vault_accounting = Some(live.clone());

        let mut reserved_left = live.reserved_liquidity;
        let mut deployed_left = live.deployed_liquidity;
        let now = Utc::now();
        for position in self.lp_positions.iter_mut().filter(|position| {
            position.asset == live.asset && position.status == PositionStatus::Active
        }) {
            let deployed = position.supplied_amount.min(deployed_left);
            deployed_left = deployed_left.saturating_sub(deployed);
            let reservable = position.supplied_amount.saturating_sub(deployed);
            let reserved = reservable.min(reserved_left);
            reserved_left = reserved_left.saturating_sub(reserved);
            let available = position.supplied_amount.saturating_sub(deployed + reserved);
            if position.deployed_amount != deployed
                || position.reserved_amount != reserved
                || position.available_amount != available
            {
                position.deployed_amount = deployed;
                position.reserved_amount = reserved;
                position.available_amount = available;
                position.updated_at = now;
                changed = true;
            }
        }
        changed
    }

    pub(super) fn live_vault_accounting(&self, asset: &str) -> Option<&LiveVaultAccounting> {
        self.live_vault_accounting
            .as_ref()
            .filter(|live| live.asset == asset)
    }
}

fn select_vault_cell(
    cells: Vec<crate::ckb_rpc::CkbLiveCell>,
    configured_out_point: Option<&str>,
) -> Result<Option<crate::ckb_rpc::CkbLiveCell>> {
    if cells.is_empty() {
        return Ok(None);
    }
    if let Some(configured) = configured_out_point
        && let Some(index) = cells
            .iter()
            .position(|cell| cell.out_point.cell_out_point() == configured)
    {
        return Ok(Some(cells.into_iter().nth(index).unwrap()));
    }
    if cells.len() == 1 {
        return Ok(cells.into_iter().next());
    }
    Err(anyhow!(
        "multiple live LiquidLane vault cells found; operator must settle the active vault pointer"
    ))
}

fn hex_bytes(value: &str) -> Result<Vec<u8>> {
    let value = value.trim_start_matches("0x");
    if value.len() % 2 != 0 {
        return Err(anyhow!("hex data must have even length"));
    }
    (0..value.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&value[index..index + 2], 16).map_err(Into::into))
        .collect()
}

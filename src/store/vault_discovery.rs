use anyhow::{Result, anyhow};

use super::{
    AppStore,
    chain_address::{address_from_script, script_from_json},
    chain_types::{parse_vault_data, required_hash},
};
use crate::domain::VaultConfig;

const VAULT_DISCOVERY_LIMIT: u32 = 10;

impl AppStore {
    pub(super) async fn discover_live_vault_config(
        &self,
        base: &VaultConfig,
    ) -> Result<Option<VaultConfig>> {
        let Some(client) = self.ckb_rpc.as_ref() else {
            return Ok(None);
        };
        let vault_lock_code = required_hash(
            base.scripts.vault_lock_code_hash.as_deref(),
            "LIQUIDLANE_VAULT_LOCK_CODE_HASH",
        )?;
        let vault_type_code = required_hash(
            base.scripts.vault_type_code_hash.as_deref(),
            "LIQUIDLANE_VAULT_TYPE_CODE_HASH",
        )?;

        let cells = client
            .live_vault_cells_by_code(&vault_type_code, &vault_lock_code, VAULT_DISCOVERY_LIMIT)
            .await?;
        let mut candidates = Vec::new();
        for cell in cells {
            let data = hex_bytes(&cell.output_data)?;
            parse_vault_data(&data)?;
            let lock_value = cell
                .output
                .get("lock")
                .ok_or_else(|| anyhow!("discovered vault cell lock is missing"))?;
            let lock = script_from_json(lock_value)?;
            let address = address_from_script(&lock, &base.network)?;
            candidates.push((address, cell.out_point.cell_out_point()));
        }

        let Some((address, cell_out_point)) = single_vault_candidate(candidates)? else {
            return Ok(None);
        };
        let mut vault = base.clone();
        vault.address = Some(address.clone());
        vault.cell_out_point = Some(cell_out_point.clone());
        vault.configured = true;
        self.persist_vault_override(address, cell_out_point).await?;
        Ok(Some(vault))
    }

    async fn persist_vault_override(&self, address: String, cell_out_point: String) -> Result<()> {
        let mut state = self.inner.write().await;
        let changed = state.vault_address.as_deref() != Some(address.as_str())
            || state.vault_cell_out_point.as_deref() != Some(cell_out_point.as_str());
        if !changed {
            return Ok(());
        }
        state.vault_address = Some(address);
        state.vault_cell_out_point = Some(cell_out_point);
        self.persist_locked(&state).await
    }
}

fn single_vault_candidate(values: Vec<(String, String)>) -> Result<Option<(String, String)>> {
    match values.len() {
        0 => Ok(None),
        1 => Ok(values.into_iter().next()),
        _ => Err(anyhow!(
            "multiple live LiquidLane vault cells found; operator must settle the active vault pointer"
        )),
    }
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

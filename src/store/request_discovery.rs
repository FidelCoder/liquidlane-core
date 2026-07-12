use std::collections::{HashMap, HashSet};

use anyhow::{Result, anyhow};
use chrono::Utc;
use uuid::Uuid;

use super::{
    AppStore,
    chain_address::{address_from_script, script_from_json},
    chain_types::{ChainScript, parse_request_data, required_hash, script_hash},
    request_recovery::{RecoveredActor, RecoveredRequest},
};
use crate::domain::{LiquidityStatus, User, UserRole, VaultConfig};

const REQUEST_DISCOVERY_LIMIT: u32 = 100;
const VAULT_DISCOVERY_LIMIT: u32 = 10;
const ARG_SEGMENT_CHARS: usize = 64;

impl AppStore {
    pub(super) async fn sync_visible_request_cells(
        &self,
        user: &User,
        vault: &VaultConfig,
    ) -> Result<()> {
        let Some(client) = self.ckb_rpc.as_ref() else {
            return Ok(());
        };
        let request_type_code = required_hash(
            vault.scripts.request_type_code_hash.as_deref(),
            "LIQUIDLANE_REQUEST_TYPE_CODE_HASH",
        )?;
        let vault_type_hashes = self.live_vault_type_hashes(vault).await?;
        let known_users = self.request_discovery_targets(user).await;
        let cells = client
            .live_cells_by_type_code(&request_type_code, REQUEST_DISCOVERY_LIMIT)
            .await?;

        let mut recovered = Vec::new();
        for cell in cells {
            let type_script = script_value(&cell.output, "type")?;
            if !request_type_belongs_to_vault(&type_script.args, &vault_type_hashes) {
                continue;
            }
            let lock = script_value(&cell.output, "lock")?;
            let lock_hash = script_hash(&lock)?;
            if !arg_segment_matches(&type_script.args, 1, &lock_hash) {
                continue;
            }
            if arg_segment(&type_script.args, 2)?
                .chars()
                .all(|ch| ch == '0')
            {
                continue;
            }
            let Some(id) = request_id_from_type_args(&type_script.args) else {
                continue;
            };
            let data = hex_bytes(&cell.output_data)?;
            let request = parse_request_data(&data)?;
            recovered.push(RecoveredRequest {
                id,
                actor: recovered_actor(id, &lock, vault, &known_users)?,
                amount: request.amount,
                lease_fee: request.lease_fee,
                duration_days: duration_days_from_expiry(request.expiry),
                request_cell_out_point: cell.out_point.cell_out_point(),
                request_tx_hash: cell.out_point.tx_hash,
                status: status_from_chain(request.status),
            });
        }

        if recovered.is_empty() {
            return Ok(());
        }
        let mut state = self.inner.write().await;
        let mut changed = false;
        for request in dedupe_recovered(recovered) {
            changed |= state.upsert_recovered_request(vault, request);
        }
        if changed {
            self.persist_locked(&state).await?;
        }
        Ok(())
    }

    async fn live_vault_type_hashes(&self, vault: &VaultConfig) -> Result<Vec<String>> {
        let Some(client) = self.ckb_rpc.as_ref() else {
            return Ok(Vec::new());
        };
        let vault_lock_code = required_hash(
            vault.scripts.vault_lock_code_hash.as_deref(),
            "LIQUIDLANE_VAULT_LOCK_CODE_HASH",
        )?;
        let vault_type_code = required_hash(
            vault.scripts.vault_type_code_hash.as_deref(),
            "LIQUIDLANE_VAULT_TYPE_CODE_HASH",
        )?;
        let cells = client
            .live_vault_cells_by_code(&vault_type_code, &vault_lock_code, VAULT_DISCOVERY_LIMIT)
            .await?;
        let mut hashes = HashSet::new();
        for cell in cells {
            let type_script = script_value(&cell.output, "type")?;
            hashes.insert(script_hash(&type_script)?);
        }
        Ok(hashes.into_iter().collect())
    }

    async fn request_discovery_targets(&self, user: &User) -> Vec<User> {
        let state = self.inner.read().await;
        let mut targets = match user.role {
            UserRole::Operator | UserRole::Lp => state
                .users
                .iter()
                .filter(|stored| matches!(stored.role, UserRole::Merchant | UserRole::Operator))
                .cloned()
                .collect::<Vec<_>>(),
            UserRole::Merchant => vec![user.clone()],
        };
        if !targets.iter().any(|stored| stored.id == user.id) {
            targets.push(user.clone());
        }
        let mut seen = HashSet::new();
        targets
            .into_iter()
            .filter(|target| seen.insert(target.ckb_address.clone()))
            .collect()
    }
}

fn recovered_actor(
    id: Uuid,
    lock: &ChainScript,
    vault: &VaultConfig,
    known_users: &[User],
) -> Result<RecoveredActor> {
    let address = address_from_script(lock, &vault.network)?;
    if let Some(user) = known_users
        .iter()
        .find(|user| user.ckb_address.eq_ignore_ascii_case(&address))
    {
        return Ok(RecoveredActor {
            id: user.id,
            display_name: user.display_name.clone(),
            ckb_address: user.ckb_address.clone(),
        });
    }
    Ok(RecoveredActor {
        id,
        display_name: format!("Recovered {}", short_address(&address)),
        ckb_address: address,
    })
}

fn request_type_belongs_to_vault(args: &str, vault_type_hashes: &[String]) -> bool {
    vault_type_hashes.is_empty()
        || vault_type_hashes
            .iter()
            .any(|hash| arg_segment_matches(args, 0, hash))
}

fn arg_segment_matches(args: &str, index: usize, expected: &str) -> bool {
    arg_segment(args, index)
        .map(|segment| segment.eq_ignore_ascii_case(expected.trim_start_matches("0x")))
        .unwrap_or(false)
}

fn request_id_from_type_args(args: &str) -> Option<Uuid> {
    uuid_from_compact_hex(arg_segment(args, 3).ok()?.get(0..32)?)
}

fn arg_segment(args: &str, index: usize) -> Result<&str> {
    let args = args.trim_start_matches("0x");
    if args.len() != ARG_SEGMENT_CHARS * 4 {
        return Err(anyhow!("capacity request cell type args are invalid"));
    }
    let start = index * ARG_SEGMENT_CHARS;
    args.get(start..start + ARG_SEGMENT_CHARS)
        .ok_or_else(|| anyhow!("capacity request cell arg segment is missing"))
}

fn uuid_from_compact_hex(value: &str) -> Option<Uuid> {
    if value.len() != 32 || !value.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return None;
    }
    Uuid::parse_str(&format!(
        "{}-{}-{}-{}-{}",
        value.get(0..8)?,
        value.get(8..12)?,
        value.get(12..16)?,
        value.get(16..20)?,
        value.get(20..32)?,
    ))
    .ok()
}

fn duration_days_from_expiry(expiry: u64) -> u16 {
    let now = Utc::now().timestamp().max(0) as u64;
    let seconds = expiry.saturating_sub(now);
    ((seconds.saturating_add(86_399) / 86_400).max(1)).min(u16::MAX as u64) as u16
}

fn status_from_chain(status: u8) -> LiquidityStatus {
    match status {
        2 => LiquidityStatus::PendingFiberChannel,
        3 => LiquidityStatus::ChannelOpen,
        4 => LiquidityStatus::Failed,
        5 => LiquidityStatus::Expired,
        6 => LiquidityStatus::Released,
        7 => LiquidityStatus::Settled,
        _ => LiquidityStatus::Requested,
    }
}

fn script_value(output: &serde_json::Value, key: &str) -> Result<ChainScript> {
    let value = output
        .get(key)
        .filter(|value| !value.is_null())
        .ok_or_else(|| anyhow!("capacity request cell {key} script is missing"))?;
    script_from_json(value)
}

fn dedupe_recovered(requests: Vec<RecoveredRequest>) -> Vec<RecoveredRequest> {
    let mut by_outpoint = HashMap::new();
    for request in requests {
        by_outpoint.insert(request.request_cell_out_point.clone(), request);
    }
    by_outpoint.into_values().collect()
}

fn short_address(address: &str) -> String {
    if address.len() <= 14 {
        return address.to_string();
    }
    format!("{}...{}", &address[..8], &address[address.len() - 6..])
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

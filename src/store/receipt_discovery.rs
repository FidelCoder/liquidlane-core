use anyhow::{Result, anyhow};
use chrono::Utc;
use uuid::Uuid;

use super::{
    AppStore, StoreState,
    accounting::receipt_cell_id,
    chain_address::script_from_json,
    chain_types::{ChainScript, parse_receipt_data, required_hash, script_from_address},
};
use crate::domain::{ActivityEvent, LpPosition, PositionStatus, User, VaultConfig};

const RECEIPT_DISCOVERY_LIMIT: u32 = 50;
const ARG_SEGMENT_CHARS: usize = 64;

impl AppStore {
    pub(super) async fn sync_user_lp_receipts(
        &self,
        user: &User,
        asset: &str,
        vault: &VaultConfig,
    ) -> Result<()> {
        let Some(client) = self.ckb_rpc.as_ref() else {
            return Ok(());
        };
        let receipt_type_code = required_hash(
            vault.scripts.lp_receipt_type_code_hash.as_deref(),
            "LIQUIDLANE_LP_RECEIPT_TYPE_CODE_HASH",
        )?;
        let user_lock = script_from_address(&user.ckb_address)?;
        let cells = client
            .live_cells_by_lock_and_type_code(
                &user_lock.code_hash,
                &user_lock.hash_type,
                &user_lock.args,
                &receipt_type_code,
                RECEIPT_DISCOVERY_LIMIT,
            )
            .await?;

        let mut recovered = Vec::new();
        for cell in cells {
            let data = hex_bytes(&cell.output_data)?;
            let receipt = parse_receipt_data(&data)?;
            if receipt.supplied == 0 {
                continue;
            }
            let type_script = cell
                .output
                .get("type")
                .filter(|value| !value.is_null())
                .ok_or_else(|| anyhow!("LP receipt cell type script is missing"))?;
            let type_script = script_from_json(type_script)?;
            let Some(receipt_id) = receipt_cell_id_from_type(&type_script) else {
                continue;
            };
            recovered.push(RecoveredReceipt {
                receipt_cell_id: receipt_id,
                receipt_cell_out_point: cell.out_point.cell_out_point(),
                supply_tx_hash: cell.out_point.tx_hash,
                supplied_amount: receipt.supplied,
                available_amount: receipt.available,
                reserved_amount: receipt.reserved,
                deployed_amount: receipt.deployed,
                fees_earned: receipt.claimed,
            });
        }

        if recovered.is_empty() {
            return Ok(());
        }

        let mut state = self.inner.write().await;
        let mut changed = false;
        for receipt in recovered {
            changed |= state.upsert_recovered_lp_receipt(user, asset, receipt);
        }
        if changed {
            self.persist_locked(&state).await?;
        }
        Ok(())
    }
}

struct RecoveredReceipt {
    receipt_cell_id: String,
    receipt_cell_out_point: String,
    supply_tx_hash: String,
    supplied_amount: u64,
    available_amount: u64,
    reserved_amount: u64,
    deployed_amount: u64,
    fees_earned: u64,
}

impl StoreState {
    fn upsert_recovered_lp_receipt(
        &mut self,
        user: &User,
        asset: &str,
        receipt: RecoveredReceipt,
    ) -> bool {
        let now = Utc::now();
        if let Some(position) = self.lp_positions.iter_mut().find(|position| {
            position.receipt_cell_id == receipt.receipt_cell_id
                || position.receipt_cell_out_point.as_deref()
                    == Some(receipt.receipt_cell_out_point.as_str())
        }) {
            let changed = position.receipt_cell_out_point.as_deref()
                != Some(receipt.receipt_cell_out_point.as_str())
                || position.supplied_amount != receipt.supplied_amount
                || position.available_amount != receipt.available_amount
                || position.reserved_amount != receipt.reserved_amount
                || position.deployed_amount != receipt.deployed_amount
                || position.fees_earned != receipt.fees_earned;
            if changed {
                position.receipt_cell_out_point = Some(receipt.receipt_cell_out_point);
                position.supply_tx_hash = receipt.supply_tx_hash;
                position.supplied_amount = receipt.supplied_amount;
                position.available_amount = receipt.available_amount;
                position.reserved_amount = receipt.reserved_amount;
                position.deployed_amount = receipt.deployed_amount;
                position.fees_earned = receipt.fees_earned;
                position.status = PositionStatus::Active;
                position.updated_at = now;
            }
            return changed;
        }

        self.events.insert(
            0,
            ActivityEvent {
                id: Uuid::new_v4(),
                actor_id: user.id,
                label: format!(
                    "{} recovered LP receipt from CKB testnet",
                    user.display_name
                ),
                amount: Some(receipt.supplied_amount),
                asset: Some(asset.to_string()),
                created_at: now,
            },
        );
        self.lp_positions.push(LpPosition {
            id: Uuid::new_v4(),
            lp_id: user.id,
            lp_name: user.display_name.clone(),
            ckb_address: user.ckb_address.clone(),
            asset: asset.to_string(),
            supplied_amount: receipt.supplied_amount,
            available_amount: receipt.available_amount,
            reserved_amount: receipt.reserved_amount,
            deployed_amount: receipt.deployed_amount,
            fees_earned: receipt.fees_earned,
            fees_claimed: 0,
            receipt_cell_id: receipt.receipt_cell_id,
            receipt_cell_out_point: Some(receipt.receipt_cell_out_point),
            supply_tx_hash: receipt.supply_tx_hash,
            status: PositionStatus::Active,
            created_at: now,
            updated_at: now,
        });
        true
    }
}

fn receipt_cell_id_from_type(type_script: &ChainScript) -> Option<String> {
    let args = type_script.args.trim_start_matches("0x");
    if args.len() < ARG_SEGMENT_CHARS {
        return None;
    }
    let intent_segment = &args[args.len() - ARG_SEGMENT_CHARS..];
    let uuid_hex = intent_segment.get(..32)?;
    let uuid = uuid_from_compact_hex(uuid_hex)?;
    Some(receipt_cell_id(uuid))
}

fn uuid_from_compact_hex(value: &str) -> Option<Uuid> {
    if value.len() != 32 || !value.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return None;
    }
    let formatted = format!(
        "{}-{}-{}-{}-{}",
        value.get(0..8)?,
        value.get(8..12)?,
        value.get(12..16)?,
        value.get(16..20)?,
        value.get(20..32)?,
    );
    Uuid::parse_str(&formatted).ok()
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

use std::collections::HashSet;

use anyhow::{Result, anyhow};
use chrono::Utc;
use uuid::Uuid;

use super::{
    AppStore, StoreState,
    accounting::request_cell_id,
    chain_address::script_from_json,
    chain_types::{parse_request_data, required_hash, script_from_address, script_hash},
};
use crate::domain::{
    ActivityEvent, CapacityReservation, LiquidityRequest, LiquidityStatus, ReservationStatus, User,
    UserRole, VaultConfig,
};

const REQUEST_DISCOVERY_LIMIT: u32 = 50;
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
        let targets = self.request_discovery_targets(user).await;
        if targets.is_empty() {
            return Ok(());
        }

        let mut recovered = Vec::new();
        for target in targets {
            let lock = script_from_address(&target.ckb_address)?;
            let lock_hash = script_hash(&lock)?;
            let cells = client
                .live_cells_by_lock_and_type_code(
                    &lock.code_hash,
                    &lock.hash_type,
                    &lock.args,
                    &request_type_code,
                    REQUEST_DISCOVERY_LIMIT,
                )
                .await?;
            for cell in cells {
                let type_script = cell
                    .output
                    .get("type")
                    .filter(|value| !value.is_null())
                    .ok_or_else(|| anyhow!("capacity request cell type script is missing"))?;
                let type_script = script_from_json(type_script)?;
                if !request_type_belongs_to_lock(&type_script.args, &lock_hash) {
                    continue;
                }
                let Some(id) = request_id_from_type_args(&type_script.args) else {
                    continue;
                };
                let data = hex_bytes(&cell.output_data)?;
                let request = parse_request_data(&data)?;
                recovered.push(RecoveredRequest {
                    id,
                    user: target.clone(),
                    amount: request.amount,
                    lease_fee: request.lease_fee,
                    duration_days: duration_days_from_expiry(request.expiry),
                    request_cell_out_point: cell.out_point.cell_out_point(),
                    request_tx_hash: cell.out_point.tx_hash,
                    status: status_from_chain(request.status),
                });
            }
        }

        if recovered.is_empty() {
            return Ok(());
        }
        let mut state = self.inner.write().await;
        let mut changed = false;
        for request in recovered {
            changed |= state.upsert_recovered_request(vault, request);
        }
        if changed {
            self.persist_locked(&state).await?;
        }
        Ok(())
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

struct RecoveredRequest {
    id: Uuid,
    user: User,
    amount: u64,
    lease_fee: u64,
    duration_days: u16,
    request_cell_out_point: String,
    request_tx_hash: String,
    status: LiquidityStatus,
}

impl StoreState {
    fn upsert_recovered_request(&mut self, vault: &VaultConfig, request: RecoveredRequest) -> bool {
        let now = Utc::now();
        let request_cell = request_cell_id(request.id);
        let mut changed = false;
        if let Some(stored) = self.liquidity_requests.iter_mut().find(|stored| {
            stored.id == request.id
                || stored.request_cell_out_point.as_deref()
                    == Some(request.request_cell_out_point.as_str())
        }) {
            if stored.request_tx_hash.as_deref() != Some(request.request_tx_hash.as_str())
                || stored.request_cell_out_point.as_deref()
                    != Some(request.request_cell_out_point.as_str())
            {
                stored.request_tx_hash = Some(request.request_tx_hash.clone());
                stored.request_cell_out_point = Some(request.request_cell_out_point.clone());
                stored.updated_at = now;
                changed = true;
            }
        } else {
            self.liquidity_requests.push(LiquidityRequest {
                id: request.id,
                merchant_id: request.user.id,
                merchant_name: request.user.display_name.clone(),
                ckb_address: request.user.ckb_address.clone(),
                asset: vault.asset.clone(),
                amount: request.amount,
                duration_days: request.duration_days,
                lease_fee: request.lease_fee,
                routing_fee_bps: 30,
                fiber_peer_pubkey: None,
                fiber_peer_address: None,
                public_channel: true,
                funding_udt_type_script: None,
                request_cell_id: request_cell.clone(),
                request_tx_hash: Some(request.request_tx_hash.clone()),
                request_cell_out_point: Some(request.request_cell_out_point.clone()),
                status: request.status.clone(),
                fiber_temporary_channel_id: None,
                channel_id: None,
                fiber_note: Some(
                    "Recovered from live CKB request cell. Reattach Fiber peer details if needed."
                        .to_string(),
                ),
                fiber_error: None,
                created_at: now,
                updated_at: now,
            });
            self.events.insert(
                0,
                ActivityEvent {
                    id: Uuid::new_v4(),
                    actor_id: request.user.id,
                    label: format!(
                        "{} recovered receive-capacity request",
                        request.user.display_name
                    ),
                    amount: Some(request.amount),
                    asset: Some(vault.asset.clone()),
                    created_at: now,
                },
            );
            changed = true;
        }

        if self
            .capacity_reservations
            .iter()
            .all(|reservation| reservation.request_id != request.id)
        {
            self.capacity_reservations.push(CapacityReservation {
                id: Uuid::new_v4(),
                request_id: request.id,
                merchant_id: request.user.id,
                merchant_name: request.user.display_name,
                ckb_address: request.user.ckb_address,
                asset: vault.asset.clone(),
                amount: request.amount,
                lease_fee: request.lease_fee,
                request_cell_id: request_cell,
                status: reservation_status(&request.status),
                created_at: now,
                updated_at: now,
            });
            changed = true;
        }
        changed
    }
}

fn request_type_belongs_to_lock(args: &str, lock_hash: &str) -> bool {
    let args = args.trim_start_matches("0x");
    let lock_hash = lock_hash.trim_start_matches("0x");
    args.len() == ARG_SEGMENT_CHARS * 4
        && args
            .get(ARG_SEGMENT_CHARS..ARG_SEGMENT_CHARS * 2)
            .is_some_and(|segment| segment.eq_ignore_ascii_case(lock_hash))
}

fn request_id_from_type_args(args: &str) -> Option<Uuid> {
    let args = args.trim_start_matches("0x");
    if args.len() != ARG_SEGMENT_CHARS * 4 {
        return None;
    }
    uuid_from_compact_hex(args.get(ARG_SEGMENT_CHARS * 3..ARG_SEGMENT_CHARS * 3 + 32)?)
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
        _ => LiquidityStatus::Requested,
    }
}

fn reservation_status(status: &LiquidityStatus) -> ReservationStatus {
    match status {
        LiquidityStatus::ChannelOpen | LiquidityStatus::PendingFiberChannel => {
            ReservationStatus::Deployed
        }
        LiquidityStatus::Failed => ReservationStatus::Failed,
        LiquidityStatus::Requested => ReservationStatus::Reserved,
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

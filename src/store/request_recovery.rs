use chrono::Utc;
use uuid::Uuid;

use super::{StoreState, accounting::request_cell_id};
use crate::domain::{
    ActivityEvent, CapacityReservation, LiquidityRequest, LiquidityStatus, ReservationStatus,
    VaultConfig,
};

#[derive(Clone)]
pub(super) struct RecoveredActor {
    pub id: Uuid,
    pub display_name: String,
    pub ckb_address: String,
}

pub(super) struct RecoveredRequest {
    pub id: Uuid,
    pub actor: RecoveredActor,
    pub amount: u64,
    pub lease_fee: u64,
    pub duration_days: u16,
    pub request_cell_out_point: String,
    pub request_tx_hash: String,
    pub status: LiquidityStatus,
}

impl StoreState {
    pub(super) fn upsert_recovered_request(
        &mut self,
        vault: &VaultConfig,
        request: RecoveredRequest,
    ) -> bool {
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
                || stored.status != request.status
            {
                stored.request_tx_hash = Some(request.request_tx_hash.clone());
                stored.request_cell_out_point = Some(request.request_cell_out_point.clone());
                stored.status = request.status.clone();
                stored.updated_at = now;
                changed = true;
            }
        } else {
            self.liquidity_requests.push(recovered_liquidity_request(
                vault,
                &request,
                &request_cell,
                now,
            ));
            self.events.insert(
                0,
                ActivityEvent {
                    id: Uuid::new_v4(),
                    actor_id: request.actor.id,
                    label: format!(
                        "{} recovered on-chain receive-capacity request",
                        request.actor.display_name
                    ),
                    amount: Some(request.amount),
                    asset: Some(vault.asset.clone()),
                    created_at: now,
                },
            );
            changed = true;
        }

        changed |= self.upsert_recovered_reservation(vault, &request, request_cell, now);
        changed
    }

    fn upsert_recovered_reservation(
        &mut self,
        vault: &VaultConfig,
        request: &RecoveredRequest,
        request_cell: String,
        now: chrono::DateTime<Utc>,
    ) -> bool {
        if let Some(stored) = self
            .capacity_reservations
            .iter_mut()
            .find(|reservation| reservation.request_id == request.id)
        {
            let status = reservation_status(&request.status);
            if stored.status != status || stored.amount != request.amount {
                stored.status = status;
                stored.amount = request.amount;
                stored.lease_fee = request.lease_fee;
                stored.updated_at = now;
                return true;
            }
            return false;
        }

        self.capacity_reservations.push(CapacityReservation {
            id: Uuid::new_v4(),
            request_id: request.id,
            merchant_id: request.actor.id,
            merchant_name: request.actor.display_name.clone(),
            ckb_address: request.actor.ckb_address.clone(),
            asset: vault.asset.clone(),
            amount: request.amount,
            lease_fee: request.lease_fee,
            request_cell_id: request_cell,
            status: reservation_status(&request.status),
            created_at: now,
            updated_at: now,
        });
        true
    }
}

fn recovered_liquidity_request(
    vault: &VaultConfig,
    request: &RecoveredRequest,
    request_cell: &str,
    now: chrono::DateTime<Utc>,
) -> LiquidityRequest {
    LiquidityRequest {
        id: request.id,
        merchant_id: request.actor.id,
        merchant_name: request.actor.display_name.clone(),
        ckb_address: request.actor.ckb_address.clone(),
        asset: vault.asset.clone(),
        amount: request.amount,
        duration_days: request.duration_days,
        lease_fee: request.lease_fee,
        routing_fee_bps: 30,
        fiber_peer_pubkey: None,
        fiber_peer_address: None,
        public_channel: true,
        funding_udt_type_script: None,
        request_cell_id: request_cell.to_string(),
        request_tx_hash: Some(request.request_tx_hash.clone()),
        request_cell_out_point: Some(request.request_cell_out_point.clone()),
        status: request.status.clone(),
        fiber_temporary_channel_id: None,
        channel_id: None,
        fiber_note: Some(
            "Recovered from a live CKB request cell. Reattach Fiber peer details before opening."
                .to_string(),
        ),
        fiber_error: None,
        created_at: now,
        updated_at: now,
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

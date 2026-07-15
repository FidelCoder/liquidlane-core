#[cfg(test)]
use super::liquidity_deploy::update_reservation_and_positions;

mod tests {
    use chrono::Utc;
    use uuid::Uuid;

    use super::*;
    use crate::domain::{
        CapacityReservation, LiquidityRequest, LiquidityStatus, LpPosition, PositionStatus,
        ReservationStatus, User, UserRole,
    };

    #[test]
    fn fiber_channel_open_moves_reserved_liquidity_to_deployed() {
        let now = Utc::now();
        let request_id = Uuid::new_v4();
        let lp_id = Uuid::new_v4();
        let merchant = user(UserRole::Merchant);
        let mut state = super::super::StoreState::default();
        state.lp_positions.push(LpPosition {
            id: Uuid::new_v4(),
            lp_id,
            lp_name: "Atlas LP".to_string(),
            ckb_address: "ckt1qlp".to_string(),
            asset: "CKB".to_string(),
            supplied_amount: 500,
            available_amount: 300,
            reserved_amount: 200,
            deployed_amount: 0,
            fees_earned: 0,
            fees_claimed: 0,
            receipt_cell_id: "ll-receipt-test".to_string(),
            receipt_cell_out_point: None,
            supply_tx_hash: "0x1111".to_string(),
            status: PositionStatus::Active,
            created_at: now,
            updated_at: now,
        });
        state.capacity_reservations.push(CapacityReservation {
            id: Uuid::new_v4(),
            request_id,
            merchant_id: merchant.id,
            merchant_name: merchant.display_name.clone(),
            ckb_address: merchant.ckb_address.clone(),
            asset: "CKB".to_string(),
            amount: 200,
            lease_fee: 1,
            request_cell_id: "ll-request-test".to_string(),
            status: ReservationStatus::Reserved,
            created_at: now,
            updated_at: now,
        });

        let pending = request(
            request_id,
            merchant.clone(),
            LiquidityStatus::PendingFiberChannel,
        );
        update_reservation_and_positions(&mut state, &pending, now);

        let position = &state.lp_positions[0];
        assert_eq!(position.available_amount, 300);
        assert_eq!(position.reserved_amount, 200);
        assert_eq!(position.deployed_amount, 0);
        assert_eq!(position.fees_earned, 0);
        assert_eq!(
            state.capacity_reservations[0].status,
            ReservationStatus::Reserved
        );

        let failed = request(request_id, merchant.clone(), LiquidityStatus::Failed);
        update_reservation_and_positions(&mut state, &failed, now);

        let position = &state.lp_positions[0];
        assert_eq!(position.available_amount, 300);
        assert_eq!(position.reserved_amount, 200);
        assert_eq!(position.deployed_amount, 0);
        assert_eq!(position.fees_earned, 0);
        assert_eq!(
            state.capacity_reservations[0].status,
            ReservationStatus::Reserved
        );

        let opened = request(request_id, merchant, LiquidityStatus::ChannelOpen);
        update_reservation_and_positions(&mut state, &opened, now);

        let position = &state.lp_positions[0];
        assert_eq!(position.available_amount, 300);
        assert_eq!(position.reserved_amount, 0);
        assert_eq!(position.deployed_amount, 200);
        assert_eq!(position.fees_earned, 0);
        assert_eq!(
            state.capacity_reservations[0].status,
            ReservationStatus::Deployed
        );
    }

    fn user(role: UserRole) -> User {
        User {
            id: Uuid::new_v4(),
            display_name: "Kairo Market".to_string(),
            ckb_address: "ckt1qmerchant".to_string(),
            wallet_type: "joyid_ckb".to_string(),
            lock_script: None,
            role,
            token: "token".to_string(),
            created_at: Utc::now(),
        }
    }

    fn request(id: Uuid, merchant: User, status: LiquidityStatus) -> LiquidityRequest {
        let now = Utc::now();
        LiquidityRequest {
            id,
            merchant_id: merchant.id,
            merchant_name: merchant.display_name,
            ckb_address: merchant.ckb_address,
            asset: "CKB".to_string(),
            amount: 200,
            usable_capacity: 0,
            duration_days: 1,
            lease_fee: 1,
            routing_fee_bps: 30,
            fiber_peer_pubkey: Some(
                "0311a6bb0683885a60518ff199394463cccab0ab48c751782c6b637e695592bb20".to_string(),
            ),
            fiber_peer_address: None,
            receiver_ckb_address: None,
            receiver_reserve_payment: 0,
            public_channel: false,
            funding_udt_type_script: None,
            request_cell_id: "ll-request-test".to_string(),
            request_tx_hash: None,
            request_cell_out_point: None,
            funding_tx_hash: None,
            funding_out_point: None,
            status,
            fiber_temporary_channel_id: None,
            channel_id: None,
            fiber_note: None,
            fiber_error: None,
            created_at: now,
            updated_at: now,
        }
    }
}

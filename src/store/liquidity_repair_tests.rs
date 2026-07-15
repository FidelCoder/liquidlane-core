#[cfg(test)]
mod tests {
    use chrono::Utc;
    use uuid::Uuid;

    use super::super::AppStore;
    use crate::domain::{
        CapacityReservation, ExecutorJob, ExecutorJobStatus, LiquidityRequest, LiquidityStatus,
        LpPosition, PositionStatus, ReleaseLiquidityRequest, ReservationStatus,
        SettleLiquidityRequest,
    };

    #[tokio::test]
    async fn release_request_returns_reserved_liquidity_to_lp_availability() {
        let store = AppStore::memory();
        let request_id = Uuid::new_v4();
        seed(
            &store,
            request(request_id, LiquidityStatus::FundingRequired),
            position(300, 200, 0),
            ReservationStatus::Reserved,
            ExecutorJobStatus::AwaitingVaultFunding,
        )
        .await;

        let updated = store
            .release_liquidity_request(
                request_id,
                ReleaseLiquidityRequest {
                    tx_hash: None,
                    signed_tx: None,
                    reason: Some("funding attempt expired".to_string()),
                },
            )
            .await
            .unwrap();

        assert_eq!(updated.status, LiquidityStatus::Released);
        let state = store.inner.read().await;
        assert_eq!(state.lp_positions[0].available_amount, 500);
        assert_eq!(state.lp_positions[0].reserved_amount, 0);
        assert_eq!(
            state.capacity_reservations[0].status,
            ReservationStatus::Released
        );
        assert_eq!(state.executor_jobs[0].status, ExecutorJobStatus::Released);
    }

    #[tokio::test]
    async fn settle_request_returns_deployed_liquidity_to_lp_availability() {
        let store = AppStore::memory();
        let request_id = Uuid::new_v4();
        seed(
            &store,
            request(request_id, LiquidityStatus::ChannelOpen),
            position(300, 0, 200),
            ReservationStatus::Deployed,
            ExecutorJobStatus::ChannelActive,
        )
        .await;

        let updated = store
            .settle_liquidity_request(
                request_id,
                SettleLiquidityRequest {
                    tx_hash: None,
                    signed_tx: None,
                    channel_id: Some("0xchannel".to_string()),
                },
            )
            .await
            .unwrap();

        assert_eq!(updated.status, LiquidityStatus::Settled);
        assert_eq!(updated.channel_id.as_deref(), Some("0xchannel"));
        let state = store.inner.read().await;
        assert_eq!(state.lp_positions[0].available_amount, 500);
        assert_eq!(state.lp_positions[0].deployed_amount, 0);
        assert_eq!(
            state.capacity_reservations[0].status,
            ReservationStatus::Released
        );
        assert_eq!(
            state.executor_jobs[0].status,
            ExecutorJobStatus::ChannelSettled
        );
    }

    async fn seed(
        store: &AppStore,
        request: LiquidityRequest,
        position: LpPosition,
        reservation_status: ReservationStatus,
        job_status: ExecutorJobStatus,
    ) {
        let now = Utc::now();
        let mut state = store.inner.write().await;
        state.lp_positions.push(position);
        state.capacity_reservations.push(CapacityReservation {
            id: Uuid::new_v4(),
            request_id: request.id,
            merchant_id: request.merchant_id,
            merchant_name: request.merchant_name.clone(),
            ckb_address: request.ckb_address.clone(),
            asset: request.asset.clone(),
            amount: request.amount,
            lease_fee: request.lease_fee,
            request_cell_id: request.request_cell_id.clone(),
            status: reservation_status,
            created_at: now,
            updated_at: now,
        });
        state.executor_jobs.push(ExecutorJob {
            id: Uuid::new_v4(),
            request_id: request.id,
            status: job_status,
            attempts: 1,
            max_retries: 3,
            last_error: None,
            fiber_ref: None,
            created_at: now,
            updated_at: now,
        });
        state.liquidity_requests.push(request);
    }

    fn position(available: u64, reserved: u64, deployed: u64) -> LpPosition {
        let now = Utc::now();
        LpPosition {
            id: Uuid::new_v4(),
            lp_id: Uuid::new_v4(),
            lp_name: "Atlas LP".to_string(),
            ckb_address: "ckt1qlp".to_string(),
            asset: "CKB".to_string(),
            supplied_amount: available + reserved + deployed,
            available_amount: available,
            reserved_amount: reserved,
            deployed_amount: deployed,
            fees_earned: 0,
            fees_claimed: 0,
            receipt_cell_id: "ll-receipt-test".to_string(),
            receipt_cell_out_point: None,
            supply_tx_hash: hash(9),
            status: PositionStatus::Active,
            created_at: now,
            updated_at: now,
        }
    }

    fn request(id: Uuid, status: LiquidityStatus) -> LiquidityRequest {
        let now = Utc::now();
        LiquidityRequest {
            id,
            merchant_id: Uuid::new_v4(),
            merchant_name: "Kairo Market".to_string(),
            ckb_address: "ckt1qmerchant".to_string(),
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
            request_tx_hash: Some(hash(1)),
            request_cell_out_point: Some(format!("{}#0x0", hash(1))),
            funding_tx_hash: None,
            funding_out_point: None,
            status,
            fiber_temporary_channel_id: Some("0xfiber-ref".to_string()),
            channel_id: None,
            fiber_note: None,
            fiber_error: None,
            created_at: now,
            updated_at: now,
        }
    }

    fn hash(byte: u8) -> String {
        format!("0x{}", format!("{byte:02x}").repeat(32))
    }
}

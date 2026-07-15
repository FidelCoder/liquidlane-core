#[cfg(test)]
mod tests {
    use chrono::Utc;
    use uuid::Uuid;

    use super::super::AppStore;
    use crate::domain::{ExternalFundingIntentStatus, LiquidityRequest, LiquidityStatus};

    #[tokio::test]
    async fn prepares_repairable_external_funding_intent() {
        let store = AppStore::memory();
        let request = request();
        {
            let mut state = store.inner.write().await;
            state.liquidity_requests.push(request.clone());
        }

        let updated = store
            .prepare_external_funding_intent(&request, request.merchant_id, true)
            .await
            .unwrap();

        assert_eq!(updated.status, LiquidityStatus::FundingRequired);
        assert!(updated.fiber_error.is_some());
        let intents = store.external_funding_intents().await;
        assert_eq!(intents.len(), 1);
        assert_eq!(intents[0].request_id, request.id);
        assert_eq!(intents[0].amount, request.amount);
        assert_eq!(
            intents[0].status,
            ExternalFundingIntentStatus::BuilderRequired
        );
    }

    #[tokio::test]
    async fn readiness_reports_builder_blocker() {
        let readiness = AppStore::memory().external_funding_readiness().await;
        assert!(!readiness.ready);
        assert!(
            readiness
                .blockers
                .iter()
                .any(|blocker| blocker.contains("transaction builder"))
        );
    }

    fn request() -> LiquidityRequest {
        let now = Utc::now();
        LiquidityRequest {
            id: Uuid::new_v4(),
            merchant_id: Uuid::new_v4(),
            merchant_name: "merchant".to_string(),
            ckb_address: "ckt1test".to_string(),
            asset: "CKB".to_string(),
            amount: 200,
            usable_capacity: 0,
            duration_days: 30,
            lease_fee: 1,
            routing_fee_bps: 30,
            fiber_peer_pubkey: Some(
                "02b6d4e3ab86a2ca2fad6fae0ecb2e1e559e0b911939872a90abdda6d20302be71".to_string(),
            ),
            fiber_peer_address: None,
            receiver_ckb_address: None,
            receiver_reserve_payment: 0,
            public_channel: false,
            funding_udt_type_script: None,
            request_cell_id: "ll-request-test".to_string(),
            request_tx_hash: Some(
                "0x1111111111111111111111111111111111111111111111111111111111111111".to_string(),
            ),
            request_cell_out_point: Some(
                "0x1111111111111111111111111111111111111111111111111111111111111111#0x0"
                    .to_string(),
            ),
            funding_tx_hash: None,
            funding_out_point: None,
            status: LiquidityStatus::Requested,
            fiber_temporary_channel_id: None,
            channel_id: None,
            fiber_note: None,
            fiber_error: None,
            created_at: now,
            updated_at: now,
        }
    }
}

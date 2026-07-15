#[cfg(test)]
mod tests {
    use chrono::Utc;
    use uuid::Uuid;

    use super::super::AppStore;
    use crate::domain::{ExternalFundingSubmitRequest, LiquidityRequest, LiquidityStatus};

    #[tokio::test]
    async fn funding_plan_reports_v2_script_blocker() {
        let store = AppStore::memory();
        let request = request(LiquidityStatus::FundingRequired);
        {
            let mut state = store.inner.write().await;
            state.liquidity_requests.push(request.clone());
        }

        let plan = store.external_funding_plan(request.id).await.unwrap();

        assert!(!plan.unsigned_tx_available);
        assert!(plan.blockers.iter().any(|item| item.contains("v1")));
        assert_eq!(plan.required_signer, "liquidlane_vault_funding_authority");
    }

    #[tokio::test]
    async fn funding_submit_refuses_when_plan_not_ready() {
        let store = AppStore::memory();
        let request = request(LiquidityStatus::FundingRequired);
        {
            let mut state = store.inner.write().await;
            state.liquidity_requests.push(request.clone());
        }

        let err = store
            .submit_external_funding_tx(
                request.id,
                ExternalFundingSubmitRequest {
                    tx_hash: hash(8),
                    funding_out_point: Some(format!("{}#0x0", hash(8))),
                    signed_tx: None,
                },
            )
            .await
            .unwrap_err();

        assert!(err.to_string().contains("Fiber RPC") || err.to_string().contains("v1"));
    }

    fn request(status: LiquidityStatus) -> LiquidityRequest {
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

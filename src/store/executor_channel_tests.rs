#[cfg(test)]
mod tests {
    use chrono::Utc;
    use uuid::Uuid;

    use super::super::executor_channel::{
        channel_matches_request, matching_failed_channel, matching_settled_channel,
        matching_usable_channel,
    };
    use crate::{domain::LiquidityStatus, fiber::FiberChannel};

    #[test]
    fn usable_channel_matches_temporary_ref() {
        let mut request = request(200);
        request.fiber_temporary_channel_id = Some("0xtemp".to_string());
        let channels = vec![channel(Some("0xtemp"), None, Some(500), true, false)];

        assert!(matching_usable_channel(&request, &[], &channels).is_some());
    }

    #[test]
    fn usable_channel_matches_handoff_ref_as_channel_id() {
        let mut request = request(207);
        request.fiber_temporary_channel_id = Some("0xhandoff".to_string());
        let channel = FiberChannel {
            channel_id: Some("0xhandoff".to_string()),
            temporary_channel_id: None,
            peer_pubkey: Some("03peer".to_string()),
            amount_ckb: Some(51),
            funding_tx_hash: None,
            funding_out_point: None,
            settlement_tx_hash: None,
            is_usable: true,
            is_closed: false,
            is_failed: false,
        };

        assert!(channel_matches_request(&request, &[], &channel));
    }

    #[test]
    fn peer_match_requires_exact_reserved_amount() {
        let request = request(200);
        let wrong_amount = channel(None, Some("03peer"), Some(100), true, false);
        let right_amount = channel(None, Some("03peer"), Some(200), true, false);

        assert!(!channel_matches_request(&request, &[], &wrong_amount));
        assert!(channel_matches_request(&request, &[], &right_amount));
    }

    #[test]
    fn channel_matches_external_funding_tx_hash() {
        let request = request(200);
        let intent = crate::domain::ExternalFundingIntent {
            id: Uuid::new_v4(),
            request_id: request.id,
            merchant_id: request.merchant_id,
            merchant_name: request.merchant_name.clone(),
            ckb_address: request.ckb_address.clone(),
            asset: request.asset.clone(),
            amount: request.amount,
            request_tx_hash: request.request_tx_hash.clone(),
            request_cell_out_point: request.request_cell_out_point.clone(),
            fiber_peer_pubkey: request.fiber_peer_pubkey.clone(),
            fiber_peer_address: request.fiber_peer_address.clone(),
            status: crate::domain::ExternalFundingIntentStatus::FundingSubmitted,
            blockers: Vec::new(),
            funding_tx_hash: Some("0xfund".to_string()),
            funding_out_point: None,
            fiber_ref: None,
            note: "submitted".to_string(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let channel = FiberChannel {
            channel_id: None,
            temporary_channel_id: None,
            peer_pubkey: None,
            amount_ckb: None,
            funding_tx_hash: Some("0xfund".to_string()),
            funding_out_point: None,
            settlement_tx_hash: None,
            is_usable: true,
            is_closed: false,
            is_failed: false,
        };

        assert!(channel_matches_request(&request, &[intent], &channel));
    }

    #[test]
    fn settled_channel_matches_open_request_by_amount() {
        let mut request = request(500);
        request.status = LiquidityStatus::ChannelOpen;
        let channels = vec![settled_channel(Some("03peer"), Some(500))];

        assert!(matching_settled_channel(&request, &[], &channels).is_some());
    }

    #[test]
    fn failed_channel_matches_pending_request_by_amount() {
        let request = request(500);
        let channels = vec![channel(None, Some("03peer"), Some(500), false, true)];

        assert!(matching_failed_channel(&request, &[], &channels).is_some());
    }

    fn request(amount: u64) -> crate::domain::LiquidityRequest {
        let now = Utc::now();
        crate::domain::LiquidityRequest {
            id: Uuid::new_v4(),
            merchant_id: Uuid::new_v4(),
            merchant_name: "Recovered merchant".to_string(),
            ckb_address: "ckt1qmerchant".to_string(),
            asset: "CKB".to_string(),
            amount,
            duration_days: 1,
            lease_fee: 1,
            routing_fee_bps: 30,
            fiber_peer_pubkey: Some("03peer".to_string()),
            fiber_peer_address: None,
            public_channel: false,
            funding_udt_type_script: None,
            request_cell_id: "ll-request-test".to_string(),
            request_tx_hash: None,
            request_cell_out_point: None,
            status: LiquidityStatus::PendingFiberChannel,
            fiber_temporary_channel_id: None,
            channel_id: None,
            fiber_note: None,
            fiber_error: None,
            created_at: now,
            updated_at: now,
        }
    }

    fn channel(
        temporary_channel_id: Option<&str>,
        peer_pubkey: Option<&str>,
        amount_ckb: Option<u64>,
        is_usable: bool,
        is_failed: bool,
    ) -> FiberChannel {
        FiberChannel {
            channel_id: None,
            temporary_channel_id: temporary_channel_id.map(str::to_string),
            peer_pubkey: peer_pubkey.map(str::to_string),
            amount_ckb,
            funding_tx_hash: None,
            funding_out_point: None,
            settlement_tx_hash: None,
            is_usable,
            is_closed: is_failed,
            is_failed,
        }
    }

    fn settled_channel(peer_pubkey: Option<&str>, amount_ckb: Option<u64>) -> FiberChannel {
        FiberChannel {
            channel_id: None,
            temporary_channel_id: None,
            peer_pubkey: peer_pubkey.map(str::to_string),
            amount_ckb,
            funding_tx_hash: None,
            funding_out_point: None,
            settlement_tx_hash: Some("0xsettle".to_string()),
            is_usable: false,
            is_closed: true,
            is_failed: false,
        }
    }
}

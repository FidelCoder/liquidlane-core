#[cfg(test)]
mod tests {
    use chrono::Utc;
    use serde_json::{Value, json};
    use uuid::Uuid;

    use super::super::{
        chain_request_payment::require_receiver_reserve_payment, chain_types::script_from_address,
    };
    use crate::domain::{LiquidityRequest, LiquidityStatus};

    const RECEIVER: &str = "ckt1qzda0cr08m85hc8jlnfp3zer7xulejywt49kt2rr0vthywaa50xwsqwh04ftzgcaymffpf245m0rjvd30x4s3rgt9e64s";

    #[test]
    fn accepts_one_exact_receiver_reserve_output() {
        assert!(require_receiver_reserve_payment(&transaction(&[201]), &request()).is_ok());
    }

    #[test]
    fn rejects_receiver_reserve_underpayment() {
        let error = require_receiver_reserve_payment(&transaction(&[200]), &request())
            .unwrap_err()
            .to_string();
        assert!(error.contains("pay exactly 201 CKB"));
    }

    #[test]
    fn rejects_duplicate_receiver_reserve_outputs() {
        let error = require_receiver_reserve_payment(&transaction(&[201, 201]), &request())
            .unwrap_err()
            .to_string();
        assert!(error.contains("pay exactly 201 CKB"));
    }

    fn transaction(amounts: &[u64]) -> Value {
        let lock = script_from_address(RECEIVER).unwrap();
        let outputs = amounts
            .iter()
            .map(|amount| {
                json!({
                    "capacity": format!("0x{:x}", u128::from(*amount) * 100_000_000),
                    "lock": {
                        "code_hash": lock.code_hash,
                        "hash_type": lock.hash_type,
                        "args": lock.args,
                    },
                    "type": null,
                })
            })
            .collect::<Vec<_>>();
        json!({
            "outputs": outputs,
            "outputs_data": amounts.iter().map(|_| "0x").collect::<Vec<_>>(),
        })
    }

    fn request() -> LiquidityRequest {
        let now = Utc::now();
        LiquidityRequest {
            id: Uuid::new_v4(),
            merchant_id: Uuid::new_v4(),
            merchant_name: "Merchant".to_string(),
            ckb_address: "ckt1qmerchant".to_string(),
            asset: "CKB".to_string(),
            amount: 200,
            usable_capacity: 0,
            duration_days: 7,
            lease_fee: 1,
            routing_fee_bps: 30,
            fiber_peer_pubkey: None,
            fiber_peer_address: None,
            receiver_ckb_address: Some(RECEIVER.to_string()),
            receiver_reserve_payment: 201,
            public_channel: false,
            funding_udt_type_script: None,
            request_cell_id: "ll-request-test".to_string(),
            request_tx_hash: None,
            request_cell_out_point: None,
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

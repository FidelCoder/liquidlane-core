use super::*;

const HASH: &str = "0x1111111111111111111111111111111111111111111111111111111111111111";
const PEER: &str = "0x2222222222222222222222222222222222222222222222222222222222222222";

#[test]
fn validates_reserve_transition() {
    let transition = VaultV2Transition::Reserve {
        before: vault(1_000, 1_000, 0, 0, 0),
        after: vault(1_000, 700, 300, 0, 3),
        request: request(300, 3, CapacityRequestV2Status::Reserved),
    };
    assert!(validate_vault_v2_transition(&transition).is_ok());
}

#[test]
fn rejects_reserve_above_available() {
    let transition = VaultV2Transition::Reserve {
        before: vault(100, 100, 0, 0, 0),
        after: vault(100, 0, 200, 0, 1),
        request: request(200, 1, CapacityRequestV2Status::Reserved),
    };
    assert!(
        validate_vault_v2_transition(&transition)
            .unwrap_err()
            .contains("reserve cannot exceed")
    );
}

#[test]
fn rejects_withdrawal_of_reserved_liquidity() {
    let transition = VaultV2Transition::Withdraw {
        before: vault(500, 200, 300, 0, 0),
        after: vault(100, 0, 100, 0, 0),
        receipt_before: receipt(500, 200, 300, 0, 0, 0),
        receipt_after: None,
        amount: 400,
    };
    assert!(
        validate_vault_v2_transition(&transition)
            .unwrap_err()
            .contains("reserved")
    );
}

#[test]
fn validates_expired_release() {
    let transition = VaultV2Transition::ReleaseExpired {
        before: vault(500, 200, 300, 0, 3),
        after: vault(500, 500, 0, 0, 3),
        request: CapacityRequestV2Data {
            expiry: 10,
            ..request(300, 3, CapacityRequestV2Status::Expired)
        },
        now: 11,
    };
    assert!(validate_vault_v2_transition(&transition).is_ok());
}

fn vault(
    total: u64,
    available: u64,
    reserved: u64,
    deployed: u64,
    fee_balance: u64,
) -> VaultV2Data {
    VaultV2Data {
        total,
        available,
        reserved,
        deployed,
        fee_balance,
        executor_key_hash: HASH.to_string(),
    }
}

fn receipt(
    supplied: u64,
    available: u64,
    reserved: u64,
    deployed: u64,
    earned: u64,
    claimed: u64,
) -> LpReceiptV2Data {
    LpReceiptV2Data {
        supplied,
        available,
        reserved,
        deployed,
        earned,
        claimed,
    }
}

fn request(amount: u64, lease_fee: u64, status: CapacityRequestV2Status) -> CapacityRequestV2Data {
    CapacityRequestV2Data {
        merchant_lock_hash: HASH.to_string(),
        amount,
        lease_fee,
        expiry: 100,
        fiber_peer_hash: PEER.to_string(),
        status,
    }
}

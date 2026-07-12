use super::*;

#[test]
fn reserve_moves_available_to_reserved_and_collects_fee() {
    let request = RequestV2 {
        amount: 200,
        lease_fee: 2,
        expiry: 100,
        status: RequestStatus::Reserved,
    };
    assert_eq!(
        validate_reserve(
            vault(500, 500, 0, 0, 0),
            vault(500, 300, 200, 0, 2),
            request
        ),
        Ok(())
    );
}

#[test]
fn execute_cannot_reduce_total() {
    let request = RequestV2 {
        amount: 200,
        lease_fee: 2,
        expiry: 100,
        status: RequestStatus::Active,
    };
    assert_eq!(
        validate_execute(
            vault(500, 300, 200, 0, 2),
            vault(400, 300, 0, 200, 2),
            request
        ),
        Err(PolicyError::BadVault)
    );
}

#[test]
fn external_funding_requires_exact_amount() {
    let request = RequestV2 {
        amount: 200,
        lease_fee: 2,
        expiry: 100,
        status: RequestStatus::Opening,
    };
    assert_eq!(
        validate_external_funding(
            vault(500, 300, 200, 0, 2),
            vault(500, 300, 0, 200, 2),
            request,
            funding(199)
        ),
        Err(PolicyError::BadDelta)
    );
    assert_eq!(
        validate_external_funding(
            vault(500, 300, 200, 0, 2),
            vault(500, 300, 0, 200, 2),
            request,
            funding(200)
        ),
        Ok(())
    );
}

#[test]
fn release_requires_expiry() {
    let request = RequestV2 {
        amount: 200,
        lease_fee: 2,
        expiry: 100,
        status: RequestStatus::Expired,
    };
    assert_eq!(
        validate_release(
            vault(500, 300, 200, 0, 2),
            vault(500, 500, 0, 0, 2),
            request,
            99
        ),
        Err(PolicyError::NotExpired)
    );
}

fn vault(total: u64, available: u64, reserved: u64, deployed: u64, fee_balance: u64) -> VaultV2 {
    VaultV2 {
        total,
        available,
        reserved,
        deployed,
        fee_balance,
    }
}

fn funding(amount: u64) -> FundingIntentV2 {
    FundingIntentV2 {
        amount,
        funding_lock_hash: [1; 32],
        shutdown_lock_hash: [2; 32],
        request_id_hash: [3; 32],
    }
}

#![allow(dead_code)]

use anyhow::{Result, anyhow};
use serde::Serialize;

use crate::domain::{
    CapacityRequestV2Data, FiberFundingIntentV2Data, LpReceiptV2Data,
    VaultExternalFundingTransition, VaultV2Data, VaultV2Transition,
    validate_external_funding_transition, validate_vault_v2_transition,
};

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum V2TxKind {
    Supply,
    Withdraw,
    ReserveCapacity,
    ReleaseExpired,
    ClaimFees,
    ExecutorFunding,
    ExternalFiberFunding,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct V2TxPlan {
    pub kind: V2TxKind,
    pub vault_delta: String,
    pub required_signer: String,
    pub dry_run_required: bool,
    pub expected_status: String,
}

pub(super) fn plan_supply_v2(
    before: VaultV2Data,
    after: VaultV2Data,
    receipt: LpReceiptV2Data,
) -> Result<V2TxPlan> {
    validate_vault_v2_transition(&VaultV2Transition::Supply {
        before: before.clone(),
        after: after.clone(),
        receipt,
    })
    .map_err(|error| anyhow!(human_v2_error(&error)))?;
    Ok(plan(
        V2TxKind::Supply,
        delta(before.total, after.total),
        "lp_wallet",
        "receipt_minted",
    ))
}

pub(super) fn plan_withdraw_v2(
    before: VaultV2Data,
    after: VaultV2Data,
    receipt_before: LpReceiptV2Data,
    receipt_after: Option<LpReceiptV2Data>,
    amount: u64,
) -> Result<V2TxPlan> {
    validate_vault_v2_transition(&VaultV2Transition::Withdraw {
        before: before.clone(),
        after: after.clone(),
        receipt_before,
        receipt_after,
        amount,
    })
    .map_err(|error| anyhow!(human_v2_error(&error)))?;
    Ok(plan(
        V2TxKind::Withdraw,
        delta(before.total, after.total),
        "lp_wallet",
        "liquidity_returned",
    ))
}

pub(super) fn plan_reserve_v2(
    before: VaultV2Data,
    after: VaultV2Data,
    request: CapacityRequestV2Data,
) -> Result<V2TxPlan> {
    validate_vault_v2_transition(&VaultV2Transition::Reserve {
        before: before.clone(),
        after: after.clone(),
        request,
    })
    .map_err(|error| anyhow!(human_v2_error(&error)))?;
    Ok(plan(
        V2TxKind::ReserveCapacity,
        delta(before.reserved, after.reserved),
        "merchant_wallet",
        "reserved",
    ))
}

pub(super) fn plan_executor_funding_v2(
    before: VaultV2Data,
    after: VaultV2Data,
    request: CapacityRequestV2Data,
) -> Result<V2TxPlan> {
    validate_vault_v2_transition(&VaultV2Transition::ExecuteOpen {
        before: before.clone(),
        after: after.clone(),
        request,
    })
    .map_err(|error| anyhow!(human_v2_error(&error)))?;
    Ok(plan(
        V2TxKind::ExecutorFunding,
        delta(before.deployed, after.deployed),
        "liquidlane_executor",
        "fiber_opening",
    ))
}

pub(super) fn plan_external_fiber_funding_v2(
    before: VaultV2Data,
    after: VaultV2Data,
    request: CapacityRequestV2Data,
    funding: FiberFundingIntentV2Data,
) -> Result<V2TxPlan> {
    validate_external_funding_transition(&VaultExternalFundingTransition {
        before: before.clone(),
        after: after.clone(),
        request,
        funding,
    })
    .map_err(|error| anyhow!(human_v2_error(&error)))?;
    Ok(plan(
        V2TxKind::ExternalFiberFunding,
        delta(before.deployed, after.deployed),
        "vault_external_funding_authority",
        "fiber_funding_tx_required",
    ))
}

pub(super) fn plan_release_expired_v2(
    before: VaultV2Data,
    after: VaultV2Data,
    request: CapacityRequestV2Data,
    now: u64,
) -> Result<V2TxPlan> {
    validate_vault_v2_transition(&VaultV2Transition::ReleaseExpired {
        before: before.clone(),
        after: after.clone(),
        request,
        now,
    })
    .map_err(|error| anyhow!(human_v2_error(&error)))?;
    Ok(plan(
        V2TxKind::ReleaseExpired,
        delta(before.available, after.available),
        "anyone_after_expiry",
        "released",
    ))
}

pub(super) fn plan_claim_fees_v2(
    before: VaultV2Data,
    after: VaultV2Data,
    receipt_before: LpReceiptV2Data,
    receipt_after: LpReceiptV2Data,
    amount: u64,
) -> Result<V2TxPlan> {
    validate_vault_v2_transition(&VaultV2Transition::ClaimFees {
        before: before.clone(),
        after: after.clone(),
        receipt_before,
        receipt_after,
        amount,
    })
    .map_err(|error| anyhow!(human_v2_error(&error)))?;
    Ok(plan(
        V2TxKind::ClaimFees,
        delta(before.fee_balance, after.fee_balance),
        "lp_wallet",
        "fees_claimed",
    ))
}

fn plan(
    kind: V2TxKind,
    vault_delta: String,
    required_signer: &str,
    expected_status: &str,
) -> V2TxPlan {
    V2TxPlan {
        kind,
        vault_delta,
        required_signer: required_signer.to_string(),
        dry_run_required: true,
        expected_status: expected_status.to_string(),
    }
}

fn delta(before: u64, after: u64) -> String {
    if after >= before {
        format!("+{}", after - before)
    } else {
        format!("-{}", before - after)
    }
}

fn human_v2_error(error: &str) -> String {
    if error.contains("available") || error.contains("exceed") {
        return "Vault has insufficient available CKB for this action".to_string();
    }
    if error.contains("expired") {
        return "This request is not expired yet, so liquidity cannot be released".to_string();
    }
    if error.contains("executor") {
        return "Executor authorization does not match this vault".to_string();
    }
    error.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::CapacityRequestV2Status;

    const HASH: &str = "0x1111111111111111111111111111111111111111111111111111111111111111";

    #[test]
    fn reserve_plan_reports_merchant_signer() {
        let plan = plan_reserve_v2(
            vault(500, 500, 0, 0, 0),
            vault(500, 300, 200, 0, 1),
            request(200, CapacityRequestV2Status::Reserved),
        )
        .unwrap();
        assert_eq!(plan.required_signer, "merchant_wallet");
        assert_eq!(plan.expected_status, "reserved");
    }

    #[test]
    fn external_fiber_funding_plan_reports_vault_authority() {
        let plan = plan_external_fiber_funding_v2(
            vault(500, 300, 200, 0, 1),
            vault(500, 300, 0, 200, 1),
            request(200, CapacityRequestV2Status::Opening),
            funding(200),
        )
        .unwrap();

        assert_eq!(plan.kind, V2TxKind::ExternalFiberFunding);
        assert_eq!(plan.required_signer, "vault_external_funding_authority");
        assert_eq!(plan.expected_status, "fiber_funding_tx_required");
    }

    #[test]
    fn release_plan_rejects_non_expired_request() {
        let err = plan_release_expired_v2(
            vault(500, 300, 200, 0, 1),
            vault(500, 500, 0, 0, 1),
            request(200, CapacityRequestV2Status::Expired),
            10,
        )
        .unwrap_err();
        assert!(err.to_string().contains("not expired"));
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

    fn funding(amount: u64) -> FiberFundingIntentV2Data {
        FiberFundingIntentV2Data {
            amount,
            funding_lock_hash: hash(4),
            shutdown_lock_hash: hash(5),
            request_id_hash: hash(6),
        }
    }

    fn hash(byte: u8) -> String {
        format!("0x{}", format!("{byte:02x}").repeat(32))
    }

    fn request(amount: u64, status: CapacityRequestV2Status) -> CapacityRequestV2Data {
        CapacityRequestV2Data {
            merchant_lock_hash: HASH.to_string(),
            amount,
            lease_fee: 1,
            expiry: 99,
            fiber_peer_hash: HASH.to_string(),
            status,
        }
    }
}

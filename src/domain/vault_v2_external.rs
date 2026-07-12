use serde::{Deserialize, Serialize};

use super::{CapacityRequestV2Data, CapacityRequestV2Status, VaultV2Data, require_vault};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct FiberFundingIntentV2Data {
    pub amount: u64,
    pub funding_lock_hash: String,
    pub shutdown_lock_hash: String,
    pub request_id_hash: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct VaultExternalFundingTransition {
    pub before: VaultV2Data,
    pub after: VaultV2Data,
    pub request: CapacityRequestV2Data,
    pub funding: FiberFundingIntentV2Data,
}

pub fn validate_external_funding_transition(
    transition: &VaultExternalFundingTransition,
) -> Result<(), String> {
    require_vault(&transition.before)?;
    require_vault(&transition.after)?;
    require_funding(&transition.funding)?;
    require(
        matches!(
            transition.request.status,
            CapacityRequestV2Status::Opening | CapacityRequestV2Status::Active
        ),
        "external funding request must be opening or active",
    )?;
    require(
        transition.funding.amount == transition.request.amount,
        "Fiber funding amount must equal reserved request amount",
    )?;
    require(
        transition.before.reserved >= transition.request.amount,
        "external funding exceeds reserved liquidity",
    )?;
    require(
        transition.after.total == transition.before.total,
        "external funding must preserve logical vault total",
    )?;
    require(
        transition.after.available == transition.before.available,
        "external funding cannot spend available liquidity",
    )?;
    require(
        transition.after.reserved == transition.before.reserved - transition.request.amount,
        "external funding must reduce reserved liquidity",
    )?;
    require(
        transition.after.deployed == transition.before.deployed + transition.request.amount,
        "external funding must increase deployed liquidity",
    )
}

fn require_funding(funding: &FiberFundingIntentV2Data) -> Result<(), String> {
    require(funding.amount > 0, "Fiber funding amount must be positive")?;
    require_hash(&funding.funding_lock_hash, "funding_lock_hash")?;
    require_hash(&funding.shutdown_lock_hash, "shutdown_lock_hash")?;
    require_hash(&funding.request_id_hash, "request_id_hash")
}

fn require_hash(value: &str, field: &str) -> Result<(), String> {
    let raw = value.strip_prefix("0x").unwrap_or(value);
    require(
        raw.len() == 64 && raw.chars().all(|ch| ch.is_ascii_hexdigit()),
        &format!("{field} must be a 32-byte hex hash"),
    )
}

fn require(condition: bool, message: &str) -> Result<(), String> {
    condition.then_some(()).ok_or_else(|| message.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_funding_moves_reserved_to_deployed() {
        let transition = VaultExternalFundingTransition {
            before: vault(1000, 650, 350, 0),
            after: vault(1000, 650, 0, 350),
            request: request(350),
            funding: funding(350),
        };

        assert_eq!(validate_external_funding_transition(&transition), Ok(()));
    }

    #[test]
    fn external_funding_rejects_wrong_amount() {
        let transition = VaultExternalFundingTransition {
            before: vault(1000, 650, 350, 0),
            after: vault(1000, 650, 0, 350),
            request: request(350),
            funding: funding(349),
        };

        assert_eq!(
            validate_external_funding_transition(&transition),
            Err("Fiber funding amount must equal reserved request amount".to_string())
        );
    }

    fn vault(total: u64, available: u64, reserved: u64, deployed: u64) -> VaultV2Data {
        VaultV2Data {
            total,
            available,
            reserved,
            deployed,
            fee_balance: 1,
            executor_key_hash: hash(1),
        }
    }

    fn request(amount: u64) -> CapacityRequestV2Data {
        CapacityRequestV2Data {
            merchant_lock_hash: hash(2),
            amount,
            lease_fee: 1,
            expiry: 100,
            fiber_peer_hash: hash(3),
            status: CapacityRequestV2Status::Opening,
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
}

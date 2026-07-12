use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct VaultV2Data {
    pub total: u64,
    pub available: u64,
    pub reserved: u64,
    pub deployed: u64,
    pub fee_balance: u64,
    pub executor_key_hash: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct LpReceiptV2Data {
    pub supplied: u64,
    pub available: u64,
    pub reserved: u64,
    pub deployed: u64,
    pub earned: u64,
    pub claimed: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CapacityRequestV2Data {
    pub merchant_lock_hash: String,
    pub amount: u64,
    pub lease_fee: u64,
    pub expiry: u64,
    pub fiber_peer_hash: String,
    pub status: CapacityRequestV2Status,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CapacityRequestV2Status {
    Reserved,
    Opening,
    Active,
    Failed,
    Expired,
    Released,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum VaultV2Transition {
    Supply {
        before: VaultV2Data,
        after: VaultV2Data,
        receipt: LpReceiptV2Data,
    },
    Withdraw {
        before: VaultV2Data,
        after: VaultV2Data,
        receipt_before: LpReceiptV2Data,
        receipt_after: Option<LpReceiptV2Data>,
        amount: u64,
    },
    Reserve {
        before: VaultV2Data,
        after: VaultV2Data,
        request: CapacityRequestV2Data,
    },
    ExecuteOpen {
        before: VaultV2Data,
        after: VaultV2Data,
        request: CapacityRequestV2Data,
    },
    ReleaseExpired {
        before: VaultV2Data,
        after: VaultV2Data,
        request: CapacityRequestV2Data,
        now: u64,
    },
    ClaimFees {
        before: VaultV2Data,
        after: VaultV2Data,
        receipt_before: LpReceiptV2Data,
        receipt_after: LpReceiptV2Data,
        amount: u64,
    },
}

pub fn validate_vault_v2_transition(transition: &VaultV2Transition) -> Result<(), String> {
    match transition {
        VaultV2Transition::Supply {
            before,
            after,
            receipt,
        } => {
            require_vault(before)?;
            require_vault(after)?;
            require_receipt(receipt)?;
            let delta = after
                .total
                .checked_sub(before.total)
                .ok_or("vault total cannot decrease on supply")?;
            require(delta > 0, "supply amount must be positive")?;
            require(
                after.available == before.available + delta,
                "supply must increase available vault liquidity",
            )?;
            require(
                receipt.supplied == delta && receipt.available == delta,
                "LP receipt must mirror supplied amount",
            )
        }
        VaultV2Transition::Withdraw {
            before,
            after,
            receipt_before,
            receipt_after,
            amount,
        } => {
            require_vault(before)?;
            require_vault(after)?;
            require_receipt(receipt_before)?;
            require(*amount > 0, "withdraw amount must be positive")?;
            require(
                receipt_before.available >= *amount,
                "cannot withdraw reserved or deployed liquidity",
            )?;
            require(
                before.available >= *amount && before.total >= *amount,
                "vault has insufficient available liquidity",
            )?;
            require(
                after.total == before.total - amount,
                "withdraw must reduce total liquidity",
            )?;
            require(
                after.available == before.available - amount,
                "withdraw must reduce available liquidity",
            )?;
            if let Some(receipt_after) = receipt_after {
                require_receipt(receipt_after)?;
                require(
                    receipt_after.supplied == receipt_before.supplied - amount,
                    "receipt supplied delta is invalid",
                )?;
                require(
                    receipt_after.available == receipt_before.available - amount,
                    "receipt available delta is invalid",
                )?;
            }
            Ok(())
        }
        VaultV2Transition::Reserve {
            before,
            after,
            request,
        } => {
            require_vault(before)?;
            require_request(request)?;
            require(
                request.status == CapacityRequestV2Status::Reserved,
                "reserve request must start reserved",
            )?;
            require(
                before.available >= request.amount,
                "reserve cannot exceed available liquidity",
            )?;
            require_vault(after)?;
            require(
                after.available == before.available - request.amount,
                "reserve must decrease available",
            )?;
            require(
                after.reserved == before.reserved + request.amount,
                "reserve must increase reserved",
            )?;
            require(
                after.fee_balance == before.fee_balance + request.lease_fee,
                "reserve must add lease fee",
            )
        }
        VaultV2Transition::ExecuteOpen {
            before,
            after,
            request,
        } => {
            require_vault(before)?;
            require_vault(after)?;
            require_request(request)?;
            require(
                matches!(
                    request.status,
                    CapacityRequestV2Status::Opening | CapacityRequestV2Status::Active
                ),
                "execute request must be opening or active",
            )?;
            require(
                before.reserved >= request.amount,
                "deployed cannot exceed reserved",
            )?;
            require(
                after.reserved == before.reserved - request.amount,
                "execute must reduce reserved",
            )?;
            require(
                after.deployed == before.deployed + request.amount,
                "execute must increase deployed",
            )
        }
        VaultV2Transition::ReleaseExpired {
            before,
            after,
            request,
            now,
        } => {
            require_vault(before)?;
            require_vault(after)?;
            require_request(request)?;
            require(*now >= request.expiry, "request has not expired")?;
            require(
                before.reserved >= request.amount,
                "release amount exceeds reserved",
            )?;
            require(
                after.reserved == before.reserved - request.amount,
                "release must reduce reserved",
            )?;
            require(
                after.available == before.available + request.amount,
                "release must return liquidity to available",
            )
        }
        VaultV2Transition::ClaimFees {
            before,
            after,
            receipt_before,
            receipt_after,
            amount,
        } => {
            require_vault(before)?;
            require_vault(after)?;
            require_receipt(receipt_before)?;
            require_receipt(receipt_after)?;
            require(*amount > 0, "claim amount must be positive")?;
            require(
                before.fee_balance >= *amount,
                "vault fee balance is insufficient",
            )?;
            require(
                receipt_before.earned >= receipt_before.claimed + amount,
                "claim exceeds earned fees",
            )?;
            require(
                after.fee_balance == before.fee_balance - amount,
                "claim must reduce fee balance",
            )?;
            require(
                receipt_after.claimed == receipt_before.claimed + amount,
                "claim must update receipt claimed fees",
            )
        }
    }
}

pub fn require_vault(vault: &VaultV2Data) -> Result<(), String> {
    require_hash(&vault.executor_key_hash, "executor_key_hash")?;
    require(
        vault.total == vault.available + vault.reserved + vault.deployed,
        "vault total must equal available + reserved + deployed",
    )
}

pub fn require_receipt(receipt: &LpReceiptV2Data) -> Result<(), String> {
    require(
        receipt.supplied == receipt.available + receipt.reserved + receipt.deployed,
        "receipt supplied must equal available + reserved + deployed",
    )?;
    require(
        receipt.earned >= receipt.claimed,
        "receipt claimed fees cannot exceed earned fees",
    )
}

pub fn require_request(request: &CapacityRequestV2Data) -> Result<(), String> {
    require(request.amount > 0, "request amount must be positive")?;
    require(request.lease_fee > 0, "request lease fee must be positive")?;
    require(request.expiry > 0, "request expiry must be set")?;
    require_hash(&request.merchant_lock_hash, "merchant_lock_hash")?;
    require_hash(&request.fiber_peer_hash, "fiber_peer_hash")
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

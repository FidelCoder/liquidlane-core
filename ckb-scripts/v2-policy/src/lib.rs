#![no_std]

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VaultV2 {
    pub total: u64,
    pub available: u64,
    pub reserved: u64,
    pub deployed: u64,
    pub fee_balance: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReceiptV2 {
    pub supplied: u64,
    pub available: u64,
    pub reserved: u64,
    pub deployed: u64,
    pub earned: u64,
    pub claimed: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RequestV2 {
    pub amount: u64,
    pub lease_fee: u64,
    pub expiry: u64,
    pub status: RequestStatus,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FundingIntentV2 {
    pub amount: u64,
    pub funding_lock_hash: [u8; 32],
    pub shutdown_lock_hash: [u8; 32],
    pub request_id_hash: [u8; 32],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RequestStatus {
    Reserved,
    Opening,
    Active,
    Failed,
    Expired,
    Released,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PolicyError {
    BadVault,
    BadReceipt,
    BadRequest,
    BadDelta,
    InsufficientAvailable,
    UnauthorizedValueMovement,
    NotExpired,
}

pub type PolicyResult = Result<(), PolicyError>;

pub fn validate_vault(vault: VaultV2) -> PolicyResult {
    require(
        vault.total == vault.available + vault.reserved + vault.deployed,
        PolicyError::BadVault,
    )
}

pub fn validate_receipt(receipt: ReceiptV2) -> PolicyResult {
    require(
        receipt.supplied == receipt.available + receipt.reserved + receipt.deployed,
        PolicyError::BadReceipt,
    )?;
    require(receipt.earned >= receipt.claimed, PolicyError::BadReceipt)
}

pub fn validate_request(request: RequestV2) -> PolicyResult {
    require(
        request.amount > 0 && request.lease_fee > 0 && request.expiry > 0,
        PolicyError::BadRequest,
    )
}

pub fn validate_supply(before: VaultV2, after: VaultV2, receipt: ReceiptV2) -> PolicyResult {
    validate_vault(before)?;
    validate_vault(after)?;
    validate_receipt(receipt)?;
    let delta = after
        .total
        .checked_sub(before.total)
        .ok_or(PolicyError::BadDelta)?;
    require(delta > 0, PolicyError::BadDelta)?;
    require(
        after.available == before.available + delta,
        PolicyError::BadDelta,
    )?;
    require(
        receipt.supplied == delta && receipt.available == delta,
        PolicyError::BadReceipt,
    )
}

pub fn validate_withdraw(
    before: VaultV2,
    after: VaultV2,
    receipt: ReceiptV2,
    amount: u64,
) -> PolicyResult {
    validate_vault(before)?;
    validate_vault(after)?;
    validate_receipt(receipt)?;
    require(amount > 0, PolicyError::BadDelta)?;
    require(
        receipt.available >= amount,
        PolicyError::InsufficientAvailable,
    )?;
    require(
        before.available >= amount && before.total >= amount,
        PolicyError::InsufficientAvailable,
    )?;
    require(after.total == before.total - amount, PolicyError::BadDelta)?;
    require(
        after.available == before.available - amount,
        PolicyError::BadDelta,
    )
}

pub fn validate_reserve(before: VaultV2, after: VaultV2, request: RequestV2) -> PolicyResult {
    validate_vault(before)?;
    validate_vault(after)?;
    validate_request(request)?;
    require(
        request.status == RequestStatus::Reserved,
        PolicyError::BadRequest,
    )?;
    require(
        before.available >= request.amount,
        PolicyError::InsufficientAvailable,
    )?;
    require(
        after.available == before.available - request.amount,
        PolicyError::BadDelta,
    )?;
    require(
        after.reserved == before.reserved + request.amount,
        PolicyError::BadDelta,
    )?;
    require(
        after.fee_balance == before.fee_balance + request.lease_fee,
        PolicyError::BadDelta,
    )
}

pub fn validate_execute(before: VaultV2, after: VaultV2, request: RequestV2) -> PolicyResult {
    validate_vault(before)?;
    validate_vault(after)?;
    validate_request(request)?;
    require(
        matches!(
            request.status,
            RequestStatus::Opening | RequestStatus::Active
        ),
        PolicyError::BadRequest,
    )?;
    require(
        before.reserved >= request.amount,
        PolicyError::InsufficientAvailable,
    )?;
    require(
        after.reserved == before.reserved - request.amount,
        PolicyError::BadDelta,
    )?;
    require(
        after.deployed == before.deployed + request.amount,
        PolicyError::BadDelta,
    )?;
    require(
        after.total == before.total,
        PolicyError::UnauthorizedValueMovement,
    )
}

pub fn validate_external_funding(
    before: VaultV2,
    after: VaultV2,
    request: RequestV2,
    funding: FundingIntentV2,
) -> PolicyResult {
    validate_execute(before, after, request)?;
    require(funding.amount == request.amount, PolicyError::BadDelta)?;
    require(
        !is_zero_hash(funding.funding_lock_hash),
        PolicyError::BadRequest,
    )?;
    require(
        !is_zero_hash(funding.shutdown_lock_hash),
        PolicyError::BadRequest,
    )?;
    require(
        !is_zero_hash(funding.request_id_hash),
        PolicyError::BadRequest,
    )
}

pub fn validate_release(
    before: VaultV2,
    after: VaultV2,
    request: RequestV2,
    now: u64,
) -> PolicyResult {
    validate_vault(before)?;
    validate_vault(after)?;
    validate_request(request)?;
    require(now >= request.expiry, PolicyError::NotExpired)?;
    require(
        before.reserved >= request.amount,
        PolicyError::InsufficientAvailable,
    )?;
    require(
        after.reserved == before.reserved - request.amount,
        PolicyError::BadDelta,
    )?;
    require(
        after.available == before.available + request.amount,
        PolicyError::BadDelta,
    )
}

pub fn validate_claim(
    before: VaultV2,
    after: VaultV2,
    receipt: ReceiptV2,
    amount: u64,
) -> PolicyResult {
    validate_vault(before)?;
    validate_vault(after)?;
    validate_receipt(receipt)?;
    require(amount > 0, PolicyError::BadDelta)?;
    require(
        before.fee_balance >= amount,
        PolicyError::InsufficientAvailable,
    )?;
    require(
        receipt.earned >= receipt.claimed + amount,
        PolicyError::InsufficientAvailable,
    )?;
    require(
        after.fee_balance == before.fee_balance - amount,
        PolicyError::BadDelta,
    )
}

fn is_zero_hash(hash: [u8; 32]) -> bool {
    hash.iter().all(|byte| *byte == 0)
}

fn require(condition: bool, error: PolicyError) -> PolicyResult {
    if condition {
        Ok(())
    } else {
        Err(error)
    }
}

#[cfg(test)]
mod tests;

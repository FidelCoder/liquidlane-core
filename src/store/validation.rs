use anyhow::{Result, anyhow};
use chrono::Utc;

use crate::domain::{
    ActivityEvent, CkbScript, CreateDepositRequest, CreateLiquidityRequest, Deposit, IntentStatus,
    User, UserRole, VaultConfig, is_plausible_ckb_address,
};

pub(super) fn ensure_vault_configured(vault: &VaultConfig) -> Result<()> {
    let address = vault.address.as_deref().unwrap_or_default().trim();
    if !vault.configured || address.is_empty() {
        return Err(anyhow!("active vault address is not configured"));
    }
    if !is_plausible_ckb_address(address) {
        return Err(anyhow!(
            "active vault address is invalid; configure LIQUIDLANE_VAULT_CKB_ADDRESS with a real CKB address"
        ));
    }
    Ok(())
}

pub(super) fn validate_pending_intent(
    status: &IntentStatus,
    expires_at: chrono::DateTime<Utc>,
) -> Result<()> {
    if status != &IntentStatus::PendingSignature {
        return Err(anyhow!("intent is not pending signature"));
    }
    if expires_at < Utc::now() {
        return Err(anyhow!("intent has expired"));
    }
    Ok(())
}

pub(super) fn validate_transaction_proof(
    tx_hash: &Option<String>,
    signed_tx: &Option<serde_json::Value>,
) -> Result<()> {
    if let Some(tx_hash) = normalize_transaction_hash(tx_hash, signed_tx).as_deref() {
        validate_tx_hash(tx_hash)?;
    }
    validate_signed_tx(signed_tx, "signed CKB transaction proof is required")?;
    Ok(())
}

pub(super) fn normalize_transaction_hash(
    tx_hash: &Option<String>,
    signed_tx: &Option<serde_json::Value>,
) -> Option<String> {
    normalize_optional(tx_hash).or_else(|| hash_from_signed_tx(signed_tx))
}

pub(super) fn is_verified_deposit(deposit: &Deposit) -> bool {
    deposit.signed_tx.is_some() && deposit.tx_hash.is_some()
}

pub(super) fn is_product_activity(event: &ActivityEvent) -> bool {
    (event.amount.is_some() && !event.label.contains("deposited vault liquidity"))
        || event.label.contains("Fiber")
        || event.label.contains("reserved")
        || event.label.contains("supplied")
        || event.label.contains("Lease fee")
}

pub(super) fn normalize_ckb_address(ckb_address: &str) -> Result<String> {
    let address = ckb_address.trim();
    if address.len() < 12 || !address.starts_with("ckt1") {
        return Err(anyhow!("ckb_address must be a valid CKB testnet address"));
    }
    Ok(address.to_string())
}

pub(super) fn normalize_wallet_type(wallet_type: &str) -> Result<String> {
    let wallet_type = wallet_type.trim().to_lowercase();
    if wallet_type.is_empty() {
        return Err(anyhow!("wallet_type is required"));
    }
    if wallet_type.len() > 32 {
        return Err(anyhow!("wallet_type is too long"));
    }
    Ok(wallet_type)
}

pub(super) fn validate_wallet_proof(
    signature: &str,
    lock_script: Option<&CkbScript>,
) -> Result<()> {
    if signature.trim().len() < 16 {
        return Err(anyhow!("CKB wallet signature proof is required"));
    }
    if let Some(script) = lock_script {
        validate_script(script)?;
    }
    Ok(())
}

pub(super) fn validate_script(script: &CkbScript) -> Result<()> {
    validate_hex_field("lock_script.code_hash", &script.code_hash, 66)?;
    validate_required("lock_script.hash_type", &script.hash_type)?;
    validate_required("lock_script.args", &script.args)?;
    if !script.args.starts_with("0x") {
        return Err(anyhow!("lock_script.args must be 0x-prefixed hex"));
    }
    Ok(())
}

pub(super) fn short_ckb_address(ckb_address: &str) -> String {
    if ckb_address.len() < 18 {
        return ckb_address.to_string();
    }
    format!(
        "{}...{}",
        &ckb_address[..8],
        &ckb_address[ckb_address.len() - 6..]
    )
}

pub(super) fn normalize_asset(asset: &str) -> String {
    asset.trim().to_uppercase()
}

pub(super) fn normalize_optional(value: &Option<String>) -> Option<String> {
    value
        .as_ref()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

pub(super) fn validate_deposit_transaction(request: &CreateDepositRequest) -> Result<()> {
    let tx_hash = normalize_deposit_tx_hash(request);
    if let Some(tx_hash) = tx_hash.as_deref() {
        validate_tx_hash(tx_hash)?;
    }
    validate_signed_tx(
        &request.signed_tx,
        "supply liquidity requires a signed CKB transaction proof",
    )?;

    let witnesses = request
        .signed_tx
        .as_ref()
        .and_then(|value| value.get("witnesses"))
        .and_then(|value| value.as_array())
        .ok_or_else(|| anyhow!("signed_tx.witnesses must be an array"))?;
    if witnesses.is_empty() {
        return Err(anyhow!("signed_tx must include at least one witness"));
    }
    Ok(())
}

pub(super) fn normalize_deposit_tx_hash(request: &CreateDepositRequest) -> Option<String> {
    normalize_optional(&request.tx_hash).or_else(|| hash_from_signed_tx(&request.signed_tx))
}

pub(super) fn validate_liquidity_request(request: &CreateLiquidityRequest) -> Result<()> {
    validate_amount(request.amount)?;
    validate_required("asset", &request.asset)?;
    if request.duration_days == 0 {
        return Err(anyhow!("duration_days must be greater than zero"));
    }
    if let Some(pubkey) = request.fiber_peer_pubkey.as_deref().map(str::trim)
        && !pubkey.is_empty()
        && !is_fiber_pubkey(pubkey)
    {
        return Err(anyhow!(
            "fiber_peer_pubkey must be a compressed 33-byte hex pubkey"
        ));
    }
    if let Some(address) = request.fiber_peer_address.as_deref().map(str::trim)
        && !address.is_empty()
        && !is_fiber_multiaddr(address)
    {
        return Err(anyhow!(
            "fiber_peer_address must be a Fiber multiaddr ending in /p2p/<peer_id>"
        ));
    }
    if let Some(script) = request.funding_udt_type_script.as_ref() {
        validate_script(script)?;
    }
    Ok(())
}

pub(super) fn validate_required(field: &str, value: &str) -> Result<()> {
    if value.trim().is_empty() {
        return Err(anyhow!("{field} is required"));
    }
    Ok(())
}

pub(super) fn validate_amount(amount: u64) -> Result<()> {
    if amount == 0 {
        return Err(anyhow!("amount must be greater than zero"));
    }
    Ok(())
}

pub(super) fn require_role(user: &User, roles: &[UserRole]) -> Result<()> {
    if roles.iter().any(|role| role == &user.role) {
        Ok(())
    } else {
        Err(anyhow!(
            "this action is not available for your account role"
        ))
    }
}

pub(super) fn lease_fee(amount: u64, duration_days: u16) -> u64 {
    let duration_multiplier = u64::from(duration_days).max(1);
    ((amount * duration_multiplier) / 3_000).max(1)
}

fn validate_signed_tx(signed_tx: &Option<serde_json::Value>, missing_message: &str) -> Result<()> {
    let signed_tx = signed_tx
        .as_ref()
        .ok_or_else(|| anyhow!("{}", missing_message))?;
    if !signed_tx.is_object() {
        return Err(anyhow!("signed_tx must be a CKB transaction object"));
    }
    for field in ["inputs", "outputs", "witnesses"] {
        let value = signed_tx
            .get(field)
            .ok_or_else(|| anyhow!("signed_tx.{field} is required"))?;
        if !value.is_array() {
            return Err(anyhow!("signed_tx.{field} must be an array"));
        }
    }
    Ok(())
}

fn hash_from_signed_tx(signed_tx: &Option<serde_json::Value>) -> Option<String> {
    signed_tx
        .as_ref()
        .and_then(|tx| tx.get("hash"))
        .and_then(|hash| hash.as_str())
        .map(str::trim)
        .filter(|hash| !hash.is_empty())
        .map(str::to_string)
}

fn validate_tx_hash(tx_hash: &str) -> Result<()> {
    validate_hex_field("tx_hash", tx_hash, 66)
}

fn validate_hex_field(field: &str, value: &str, expected_len: usize) -> Result<()> {
    let value = value.trim();
    if value.len() != expected_len || !value.starts_with("0x") {
        return Err(anyhow!(
            "{field} must be 0x-prefixed hex with expected length"
        ));
    }
    if !value[2..].chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err(anyhow!("{field} must be hex"));
    }
    Ok(())
}

fn is_fiber_pubkey(pubkey: &str) -> bool {
    let raw = pubkey.strip_prefix("0x").unwrap_or(pubkey);
    raw.len() == 66 && raw.chars().all(|ch| ch.is_ascii_hexdigit())
}

fn is_fiber_multiaddr(address: &str) -> bool {
    address.len() <= 512
        && address.starts_with('/')
        && address.contains("/p2p/")
        && !address.chars().any(char::is_whitespace)
}

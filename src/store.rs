use std::{collections::HashSet, path::PathBuf};

use anyhow::{Result, anyhow};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::{
    domain::{
        ActivityEvent, AuthChallenge, AuthResponse, ChallengeRequest, ChallengeResponse, CkbScript,
        CreateDepositRequest, CreateLiquidityRequest, Dashboard, Deposit, LiquidityQuote,
        LiquidityRequest, LiquidityStatus, User, UserProfile, UserRole, VaultSummary,
        VerifyWalletRequest,
    },
    fiber::FiberClient,
};

pub struct AppStore {
    path: PathBuf,
    fiber: FiberClient,
    inner: RwLock<StoreState>,
}

#[derive(Default, Serialize, Deserialize)]
struct StoreState {
    users: Vec<User>,
    challenges: Vec<AuthChallenge>,
    deposits: Vec<Deposit>,
    liquidity_requests: Vec<LiquidityRequest>,
    events: Vec<ActivityEvent>,
}

impl AppStore {
    pub async fn load(path: PathBuf, fiber: FiberClient) -> Result<Self> {
        let state = match tokio::fs::read_to_string(&path).await {
            Ok(contents) => serde_json::from_str(&contents)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => StoreState::default(),
            Err(error) => return Err(error.into()),
        };

        Ok(Self {
            path,
            fiber,
            inner: RwLock::new(state),
        })
    }

    pub fn memory() -> Self {
        Self {
            path: std::env::temp_dir().join(format!("liquidlane-test-{}.json", Uuid::new_v4())),
            fiber: FiberClient::disabled(),
            inner: RwLock::new(StoreState::default()),
        }
    }

    pub async fn create_challenge(&self, request: ChallengeRequest) -> Result<ChallengeResponse> {
        let ckb_address = normalize_ckb_address(&request.ckb_address)?;
        let wallet_type = normalize_wallet_type(&request.wallet_type)?;
        let now = Utc::now();
        let expires_at = now + Duration::minutes(5);
        let challenge_id = Uuid::new_v4();
        let nonce = Uuid::new_v4();
        let message = format!(
            "LiquidLane CKB wallet sign-in

CKB address: {ckb_address}
Wallet: {wallet_type}
Role: {:?}
Challenge: {challenge_id}
Nonce: {nonce}
Expires: {}

Only sign this message for LiquidLane.",
            request.role,
            expires_at.to_rfc3339()
        );

        let challenge = AuthChallenge {
            id: challenge_id,
            ckb_address,
            wallet_type,
            role: request.role,
            message: message.clone(),
            expires_at,
            consumed: false,
        };

        let mut state = self.inner.write().await;
        state.challenges.push(challenge);
        self.persist_locked(&state).await?;

        Ok(ChallengeResponse {
            challenge_id,
            message,
            expires_at,
        })
    }

    pub async fn verify_wallet(&self, request: VerifyWalletRequest) -> Result<AuthResponse> {
        let ckb_address = normalize_ckb_address(&request.ckb_address)?;
        let wallet_type = normalize_wallet_type(&request.wallet_type)?;
        validate_wallet_proof(&request.signature, request.lock_script.as_ref())?;

        let mut state = self.inner.write().await;
        let role = {
            let challenge = state
                .challenges
                .iter_mut()
                .find(|challenge| challenge.id == request.challenge_id)
                .ok_or_else(|| anyhow!("challenge not found"))?;

            if challenge.consumed {
                return Err(anyhow!("challenge has already been used"));
            }
            if challenge.expires_at < Utc::now() {
                return Err(anyhow!("challenge has expired"));
            }
            if challenge.ckb_address != ckb_address {
                return Err(anyhow!(
                    "challenge CKB address does not match request address"
                ));
            }
            if challenge.wallet_type != wallet_type {
                return Err(anyhow!(
                    "challenge wallet type does not match request wallet"
                ));
            }

            challenge.consumed = true;
            challenge.role.clone()
        };

        let now = Utc::now();
        let display_name = request
            .display_name
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| short_ckb_address(&ckb_address));
        let user_index = state
            .users
            .iter()
            .position(|user| user.ckb_address == ckb_address && user.wallet_type == wallet_type);

        let user = match user_index {
            Some(index) => {
                state.users[index].display_name = display_name.trim().to_string();
                state.users[index].role = role;
                state.users[index].token = Uuid::new_v4().to_string();
                state.users[index].lock_script = request.lock_script.clone();
                state.users[index].clone()
            }
            None => {
                let user = User {
                    id: Uuid::new_v4(),
                    display_name: display_name.trim().to_string(),
                    ckb_address: ckb_address.clone(),
                    wallet_type: wallet_type.clone(),
                    lock_script: request.lock_script.clone(),
                    role,
                    token: Uuid::new_v4().to_string(),
                    created_at: now,
                };
                state.users.push(user.clone());
                user
            }
        };

        state.events.insert(
            0,
            ActivityEvent {
                id: Uuid::new_v4(),
                actor_id: user.id,
                label: format!("{} authenticated CKB wallet", user.display_name),
                amount: None,
                asset: None,
                created_at: now,
            },
        );
        self.persist_locked(&state).await?;

        Ok(AuthResponse {
            token: user.token.clone(),
            user: UserProfile::from(&user),
        })
    }

    pub async fn user_by_token(&self, token: &str) -> Option<User> {
        self.inner
            .read()
            .await
            .users
            .iter()
            .find(|user| user.token == token)
            .cloned()
    }

    pub async fn dashboard(&self, user: &User, asset: Option<String>) -> Dashboard {
        let state = self.inner.read().await;
        let asset = asset
            .unwrap_or_else(|| "USDC".to_string())
            .trim()
            .to_uppercase();
        Dashboard {
            user: UserProfile::from(user),
            vault: state.vault_summary(asset),
            deposits: state.visible_deposits(user),
            liquidity_requests: state.visible_liquidity_requests(user),
            activity: state.visible_activity(user),
        }
    }

    pub async fn quote(
        &self,
        user: &User,
        request: &CreateLiquidityRequest,
    ) -> Result<LiquidityQuote> {
        require_role(user, &[UserRole::Merchant, UserRole::Operator])?;
        validate_liquidity_request(request)?;

        let asset = normalize_asset(&request.asset);
        let available_liquidity = self
            .inner
            .read()
            .await
            .vault_summary(asset.clone())
            .available_liquidity;

        Ok(LiquidityQuote {
            asset,
            amount: request.amount,
            duration_days: request.duration_days,
            lease_fee: lease_fee(request.amount, request.duration_days),
            routing_fee_bps: 30,
            available: available_liquidity >= request.amount,
            available_liquidity,
        })
    }

    pub async fn create_deposit(
        &self,
        user: &User,
        request: CreateDepositRequest,
    ) -> Result<Deposit> {
        require_role(user, &[UserRole::Lp, UserRole::Operator])?;
        validate_amount(request.amount)?;
        validate_required("asset", &request.asset)?;

        let deposit = Deposit {
            id: Uuid::new_v4(),
            lp_id: user.id,
            lp_name: user.display_name.clone(),
            ckb_address: user.ckb_address.clone(),
            asset: normalize_asset(&request.asset),
            amount: request.amount,
            tx_hash: request.tx_hash,
            created_at: Utc::now(),
        };

        let mut state = self.inner.write().await;
        state.events.insert(
            0,
            ActivityEvent {
                id: Uuid::new_v4(),
                actor_id: user.id,
                label: format!("{} deposited vault liquidity", user.display_name),
                amount: Some(deposit.amount),
                asset: Some(deposit.asset.clone()),
                created_at: deposit.created_at,
            },
        );
        state.deposits.push(deposit.clone());
        self.persist_locked(&state).await?;

        Ok(deposit)
    }

    pub async fn create_liquidity_request(
        &self,
        user: &User,
        request: CreateLiquidityRequest,
    ) -> Result<LiquidityRequest> {
        require_role(user, &[UserRole::Merchant, UserRole::Operator])?;
        let quote = self.quote(user, &request).await?;
        if !quote.available {
            return Err(anyhow!(
                "only {} {} is available; deposit more liquidity before requesting {} {}",
                quote.available_liquidity,
                quote.asset,
                request.amount,
                quote.asset
            ));
        }

        let now = Utc::now();
        let liquidity_request = LiquidityRequest {
            id: Uuid::new_v4(),
            merchant_id: user.id,
            merchant_name: user.display_name.clone(),
            ckb_address: user.ckb_address.clone(),
            asset: quote.asset,
            amount: request.amount,
            duration_days: request.duration_days,
            lease_fee: quote.lease_fee,
            routing_fee_bps: quote.routing_fee_bps,
            fiber_peer_pubkey: normalize_optional(&request.fiber_peer_pubkey),
            public_channel: request.public_channel.unwrap_or(true),
            funding_udt_type_script: request.funding_udt_type_script,
            status: LiquidityStatus::Requested,
            fiber_temporary_channel_id: None,
            channel_id: None,
            fiber_note: None,
            fiber_error: None,
            created_at: now,
            updated_at: now,
        };

        let mut state = self.inner.write().await;
        if state
            .vault_summary(liquidity_request.asset.clone())
            .available_liquidity
            < liquidity_request.amount
        {
            return Err(anyhow!("liquidity was just reserved by another request"));
        }
        state.events.insert(
            0,
            ActivityEvent {
                id: Uuid::new_v4(),
                actor_id: user.id,
                label: format!("{} reserved receive capacity", user.display_name),
                amount: Some(liquidity_request.amount),
                asset: Some(liquidity_request.asset.clone()),
                created_at: now,
            },
        );
        state.liquidity_requests.push(liquidity_request.clone());
        self.persist_locked(&state).await?;

        Ok(liquidity_request)
    }

    pub async fn deploy_liquidity(&self, user: &User, id: Uuid) -> Result<LiquidityRequest> {
        require_role(user, &[UserRole::Merchant, UserRole::Operator])?;
        let request = {
            let state = self.inner.read().await;
            let request = state
                .liquidity_requests
                .iter()
                .find(|request| request.id == id)
                .ok_or_else(|| anyhow!("liquidity request not found"))?;

            if user.role != UserRole::Operator && request.merchant_id != user.id {
                return Err(anyhow!("you can only open your own liquidity requests"));
            }
            if request.status == LiquidityStatus::ChannelOpen {
                return Ok(request.clone());
            }
            request.clone()
        };

        let outcome = self.fiber.open_channel(&request).await;

        let mut state = self.inner.write().await;
        let request = state
            .liquidity_requests
            .iter_mut()
            .find(|request| request.id == id)
            .ok_or_else(|| anyhow!("liquidity request not found"))?;

        let now = Utc::now();
        let event_label;
        match outcome {
            Ok(outcome) => {
                request.status = LiquidityStatus::PendingFiberChannel;
                request.fiber_temporary_channel_id = outcome.temporary_channel_id;
                request.channel_id = outcome.channel_id;
                request.fiber_note = outcome.note;
                request.fiber_error = None;
                request.updated_at = now;
                event_label = if outcome.rpc_submitted {
                    format!("Submitted Fiber open_channel for {}", request.merchant_name)
                } else {
                    format!("Queued Fiber channel open for {}", request.merchant_name)
                };
            }
            Err(error) => {
                request.status = LiquidityStatus::Failed;
                request.fiber_error = Some(error.to_string());
                request.fiber_note = None;
                request.updated_at = now;
                event_label = format!("Fiber channel open failed for {}", request.merchant_name);
            }
        }

        let updated = request.clone();
        state.events.insert(
            0,
            ActivityEvent {
                id: Uuid::new_v4(),
                actor_id: user.id,
                label: event_label,
                amount: Some(updated.amount),
                asset: Some(updated.asset.clone()),
                created_at: now,
            },
        );
        self.persist_locked(&state).await?;

        Ok(updated)
    }

    async fn persist_locked(&self, state: &StoreState) -> Result<()> {
        if let Some(parent) = self
            .path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(&self.path, serde_json::to_string_pretty(state)?).await?;
        Ok(())
    }
}

impl StoreState {
    fn vault_summary(&self, asset: String) -> VaultSummary {
        let total_deposits = self
            .deposits
            .iter()
            .filter(|deposit| deposit.asset == asset)
            .map(|deposit| deposit.amount)
            .sum::<u64>();
        let reserved_liquidity = self
            .liquidity_requests
            .iter()
            .filter(|request| {
                request.asset == asset && request.status == LiquidityStatus::Requested
            })
            .map(|request| request.amount)
            .sum::<u64>();
        let pending_channel_liquidity = self
            .liquidity_requests
            .iter()
            .filter(|request| {
                request.asset == asset && request.status == LiquidityStatus::PendingFiberChannel
            })
            .map(|request| request.amount)
            .sum::<u64>();
        let deployed_liquidity = self
            .liquidity_requests
            .iter()
            .filter(|request| {
                request.asset == asset && request.status == LiquidityStatus::ChannelOpen
            })
            .map(|request| request.amount)
            .sum::<u64>();
        let fees_earned = self
            .liquidity_requests
            .iter()
            .filter(|request| {
                request.asset == asset && request.status == LiquidityStatus::ChannelOpen
            })
            .map(|request| request.lease_fee)
            .sum::<u64>();
        let lp_count = self
            .deposits
            .iter()
            .filter(|deposit| deposit.asset == asset)
            .map(|deposit| deposit.lp_id)
            .collect::<HashSet<_>>()
            .len();
        let active_requests = self
            .liquidity_requests
            .iter()
            .filter(|request| {
                request.asset == asset
                    && matches!(
                        request.status,
                        LiquidityStatus::Requested | LiquidityStatus::PendingFiberChannel
                    )
            })
            .count();
        let used = reserved_liquidity + pending_channel_liquidity + deployed_liquidity;

        VaultSummary {
            asset,
            total_deposits,
            reserved_liquidity,
            pending_channel_liquidity,
            deployed_liquidity,
            available_liquidity: total_deposits.saturating_sub(used),
            fees_earned,
            lp_count,
            active_requests,
        }
    }

    fn visible_deposits(&self, user: &User) -> Vec<Deposit> {
        match user.role {
            UserRole::Operator | UserRole::Merchant => self.deposits.clone(),
            UserRole::Lp => self
                .deposits
                .iter()
                .filter(|deposit| deposit.lp_id == user.id)
                .cloned()
                .collect(),
        }
    }

    fn visible_liquidity_requests(&self, user: &User) -> Vec<LiquidityRequest> {
        match user.role {
            UserRole::Operator | UserRole::Lp => self.liquidity_requests.clone(),
            UserRole::Merchant => self
                .liquidity_requests
                .iter()
                .filter(|request| request.merchant_id == user.id)
                .cloned()
                .collect(),
        }
    }

    fn visible_activity(&self, user: &User) -> Vec<ActivityEvent> {
        self.events
            .iter()
            .filter(|event| {
                user.role == UserRole::Operator
                    || event.actor_id == user.id
                    || event.label.contains("Lease fee")
            })
            .take(30)
            .cloned()
            .collect()
    }
}

fn normalize_ckb_address(ckb_address: &str) -> Result<String> {
    let address = ckb_address.trim();
    if address.len() < 12 || !(address.starts_with("ckb") || address.starts_with("ckt")) {
        return Err(anyhow!(
            "ckb_address must be a valid CKB mainnet or testnet address"
        ));
    }
    Ok(address.to_string())
}

fn normalize_wallet_type(wallet_type: &str) -> Result<String> {
    let wallet_type = wallet_type.trim().to_lowercase();
    if wallet_type.is_empty() {
        return Err(anyhow!("wallet_type is required"));
    }
    if wallet_type.len() > 32 {
        return Err(anyhow!("wallet_type is too long"));
    }
    Ok(wallet_type)
}

fn validate_wallet_proof(signature: &str, lock_script: Option<&CkbScript>) -> Result<()> {
    if signature.trim().len() < 16 {
        return Err(anyhow!("CKB wallet signature proof is required"));
    }
    if let Some(script) = lock_script {
        validate_script(script)?;
    }
    Ok(())
}

fn validate_script(script: &CkbScript) -> Result<()> {
    validate_hex_field("lock_script.code_hash", &script.code_hash, 66)?;
    validate_required("lock_script.hash_type", &script.hash_type)?;
    validate_required("lock_script.args", &script.args)?;
    if !script.args.starts_with("0x") {
        return Err(anyhow!("lock_script.args must be 0x-prefixed hex"));
    }
    Ok(())
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

fn short_ckb_address(ckb_address: &str) -> String {
    if ckb_address.len() < 18 {
        return ckb_address.to_string();
    }
    format!(
        "{}...{}",
        &ckb_address[..8],
        &ckb_address[ckb_address.len() - 6..]
    )
}

fn normalize_asset(asset: &str) -> String {
    asset.trim().to_uppercase()
}

fn normalize_optional(value: &Option<String>) -> Option<String> {
    value
        .as_ref()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn validate_liquidity_request(request: &CreateLiquidityRequest) -> Result<()> {
    validate_amount(request.amount)?;
    validate_required("asset", &request.asset)?;
    if request.duration_days == 0 {
        return Err(anyhow!("duration_days must be greater than zero"));
    }
    if let Some(pubkey) = request.fiber_peer_pubkey.as_deref().map(str::trim) {
        if !pubkey.is_empty() && !is_fiber_pubkey(pubkey) {
            return Err(anyhow!(
                "fiber_peer_pubkey must be a compressed 33-byte hex pubkey"
            ));
        }
    }
    if let Some(script) = request.funding_udt_type_script.as_ref() {
        validate_script(script)?;
    }
    Ok(())
}

fn is_fiber_pubkey(pubkey: &str) -> bool {
    let raw = pubkey.strip_prefix("0x").unwrap_or(pubkey);
    raw.len() == 66 && raw.chars().all(|ch| ch.is_ascii_hexdigit())
}

fn validate_required(field: &str, value: &str) -> Result<()> {
    if value.trim().is_empty() {
        return Err(anyhow!("{field} is required"));
    }
    Ok(())
}

fn validate_amount(amount: u64) -> Result<()> {
    if amount == 0 {
        return Err(anyhow!("amount must be greater than zero"));
    }
    Ok(())
}

fn require_role(user: &User, roles: &[UserRole]) -> Result<()> {
    if roles.iter().any(|role| role == &user.role) {
        Ok(())
    } else {
        Err(anyhow!(
            "this action is not available for your account role"
        ))
    }
}

fn lease_fee(amount: u64, duration_days: u16) -> u64 {
    let duration_multiplier = u64::from(duration_days).max(1);
    ((amount * duration_multiplier) / 3_000).max(1)
}

use std::{collections::HashSet, path::PathBuf};

use anyhow::{Result, anyhow};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::domain::{
    ActivityEvent, AuthRequest, AuthResponse, CreateDepositRequest, CreateLiquidityRequest,
    Dashboard, Deposit, LiquidityQuote, LiquidityRequest, LiquidityStatus, User, UserProfile,
    UserRole, VaultSummary,
};

pub struct AppStore {
    path: PathBuf,
    inner: RwLock<StoreState>,
}

#[derive(Default, Serialize, Deserialize)]
struct StoreState {
    users: Vec<User>,
    deposits: Vec<Deposit>,
    liquidity_requests: Vec<LiquidityRequest>,
    events: Vec<ActivityEvent>,
}

impl AppStore {
    pub async fn load(path: PathBuf) -> Result<Self> {
        let state = match tokio::fs::read_to_string(&path).await {
            Ok(contents) => serde_json::from_str(&contents)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => StoreState::default(),
            Err(error) => return Err(error.into()),
        };

        Ok(Self {
            path,
            inner: RwLock::new(state),
        })
    }

    pub fn memory() -> Self {
        Self {
            path: std::env::temp_dir().join(format!("liquidlane-test-{}.json", Uuid::new_v4())),
            inner: RwLock::new(StoreState::default()),
        }
    }

    pub async fn auth(&self, request: AuthRequest) -> Result<AuthResponse> {
        validate_required("name", &request.name)?;
        validate_required("email", &request.email)?;

        let mut state = self.inner.write().await;
        let normalized_email = request.email.trim().to_lowercase();
        let now = Utc::now();
        let user_index = state
            .users
            .iter()
            .position(|user| user.email == normalized_email);

        let user = match user_index {
            Some(index) => {
                state.users[index].name = request.name.trim().to_string();
                state.users[index].role = request.role;
                state.users[index].clone()
            }
            None => {
                let user = User {
                    id: Uuid::new_v4(),
                    name: request.name.trim().to_string(),
                    email: normalized_email,
                    role: request.role,
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
                label: format!("{} signed in", user.name),
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
            lp_name: user.name.clone(),
            asset: normalize_asset(&request.asset),
            amount: request.amount,
            created_at: Utc::now(),
        };

        let mut state = self.inner.write().await;
        state.events.insert(
            0,
            ActivityEvent {
                id: Uuid::new_v4(),
                actor_id: user.id,
                label: format!("{} deposited liquidity", user.name),
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
            merchant_name: user.name.clone(),
            asset: quote.asset,
            amount: request.amount,
            duration_days: request.duration_days,
            lease_fee: quote.lease_fee,
            routing_fee_bps: quote.routing_fee_bps,
            status: LiquidityStatus::Requested,
            channel_id: None,
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
                label: format!("{} reserved receive capacity", user.name),
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
        let mut state = self.inner.write().await;
        let request = state
            .liquidity_requests
            .iter_mut()
            .find(|request| request.id == id)
            .ok_or_else(|| anyhow!("liquidity request not found"))?;

        if user.role != UserRole::Operator && request.merchant_id != user.id {
            return Err(anyhow!("you can only deploy your own liquidity requests"));
        }

        if request.status == LiquidityStatus::Deployed {
            return Ok(request.clone());
        }

        request.status = LiquidityStatus::Deployed;
        request.channel_id = Some(format!("fiber-channel-{}", &id.to_string()[..8]));
        request.updated_at = Utc::now();
        let updated = request.clone();

        state.events.insert(
            0,
            ActivityEvent {
                id: Uuid::new_v4(),
                actor_id: user.id,
                label: format!("Deployed channel capacity for {}", updated.merchant_name),
                amount: Some(updated.amount),
                asset: Some(updated.asset.clone()),
                created_at: updated.updated_at,
            },
        );
        state.events.insert(
            0,
            ActivityEvent {
                id: Uuid::new_v4(),
                actor_id: user.id,
                label: "Lease fee distributed to LP vault".to_string(),
                amount: Some(updated.lease_fee),
                asset: Some(updated.asset.clone()),
                created_at: updated.updated_at,
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
        let deployed_liquidity = self
            .liquidity_requests
            .iter()
            .filter(|request| request.asset == asset && request.status == LiquidityStatus::Deployed)
            .map(|request| request.amount)
            .sum::<u64>();
        let fees_earned = self
            .liquidity_requests
            .iter()
            .filter(|request| request.asset == asset && request.status == LiquidityStatus::Deployed)
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
                request.asset == asset && request.status == LiquidityStatus::Requested
            })
            .count();
        let used = reserved_liquidity + deployed_liquidity;

        VaultSummary {
            asset,
            total_deposits,
            reserved_liquidity,
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

fn normalize_asset(asset: &str) -> String {
    asset.trim().to_uppercase()
}

fn validate_liquidity_request(request: &CreateLiquidityRequest) -> Result<()> {
    validate_amount(request.amount)?;
    validate_required("asset", &request.asset)?;
    if request.duration_days == 0 {
        return Err(anyhow!("duration_days must be greater than zero"));
    }
    Ok(())
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

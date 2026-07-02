use std::collections::HashSet;

use anyhow::{Result, anyhow};
use chrono::Utc;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::domain::{
    ActivityEvent, CreateDepositRequest, CreateLiquidityRequest, Deposit, LiquidityQuote,
    LiquidityRequest, LiquidityStatus, VaultSummary,
};

pub struct AppStore {
    inner: RwLock<StoreState>,
}

#[derive(Default)]
struct StoreState {
    deposits: Vec<Deposit>,
    liquidity_requests: Vec<LiquidityRequest>,
    events: Vec<ActivityEvent>,
}

impl AppStore {
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(StoreState::seeded()),
        }
    }

    pub async fn vault_summary(&self, asset: Option<String>) -> VaultSummary {
        let state = self.inner.read().await;
        state.vault_summary(asset.unwrap_or_else(|| "USDC".to_string()))
    }

    pub async fn deposits(&self) -> Vec<Deposit> {
        self.inner.read().await.deposits.clone()
    }

    pub async fn create_deposit(&self, request: CreateDepositRequest) -> Result<Deposit> {
        validate_amount(request.amount)?;
        validate_required("lp_name", &request.lp_name)?;
        validate_required("asset", &request.asset)?;

        let deposit = Deposit {
            id: Uuid::new_v4(),
            lp_name: request.lp_name,
            asset: normalize_asset(&request.asset),
            amount: request.amount,
            created_at: Utc::now(),
        };

        let mut state = self.inner.write().await;
        state.events.insert(
            0,
            ActivityEvent {
                id: Uuid::new_v4(),
                label: format!("{} deposited liquidity", deposit.lp_name),
                amount: Some(deposit.amount),
                asset: Some(deposit.asset.clone()),
                created_at: deposit.created_at,
            },
        );
        state.deposits.push(deposit.clone());

        Ok(deposit)
    }

    pub async fn quote(&self, request: &CreateLiquidityRequest) -> Result<LiquidityQuote> {
        validate_amount(request.amount)?;
        validate_required("merchant_name", &request.merchant_name)?;
        validate_required("asset", &request.asset)?;
        if request.duration_days == 0 {
            return Err(anyhow!("duration_days must be greater than zero"));
        }

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
        })
    }

    pub async fn liquidity_requests(&self) -> Vec<LiquidityRequest> {
        self.inner.read().await.liquidity_requests.clone()
    }

    pub async fn create_liquidity_request(
        &self,
        request: CreateLiquidityRequest,
    ) -> Result<LiquidityRequest> {
        let quote = self.quote(&request).await?;
        if !quote.available {
            return Err(anyhow!("not enough available liquidity for this request"));
        }

        let now = Utc::now();
        let liquidity_request = LiquidityRequest {
            id: Uuid::new_v4(),
            merchant_name: request.merchant_name,
            asset: quote.asset,
            amount: request.amount,
            duration_days: request.duration_days,
            lease_fee: quote.lease_fee,
            status: LiquidityStatus::Requested,
            channel_id: None,
            created_at: now,
            updated_at: now,
        };

        let mut state = self.inner.write().await;
        state.events.insert(
            0,
            ActivityEvent {
                id: Uuid::new_v4(),
                label: format!(
                    "{} requested receive capacity",
                    liquidity_request.merchant_name
                ),
                amount: Some(liquidity_request.amount),
                asset: Some(liquidity_request.asset.clone()),
                created_at: now,
            },
        );
        state.liquidity_requests.push(liquidity_request.clone());

        Ok(liquidity_request)
    }

    pub async fn deploy_liquidity(&self, id: Uuid) -> Result<LiquidityRequest> {
        let mut state = self.inner.write().await;
        let request = state
            .liquidity_requests
            .iter_mut()
            .find(|request| request.id == id)
            .ok_or_else(|| anyhow!("liquidity request not found"))?;

        request.status = LiquidityStatus::Deployed;
        request.channel_id = Some(format!("fiber-channel-{}", &id.to_string()[..8]));
        request.updated_at = Utc::now();
        let updated = request.clone();

        state.events.insert(
            0,
            ActivityEvent {
                id: Uuid::new_v4(),
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
                label: "Lease fee distributed to LP vault".to_string(),
                amount: Some(updated.lease_fee),
                asset: Some(updated.asset.clone()),
                created_at: updated.updated_at,
            },
        );

        Ok(updated)
    }

    pub async fn activity(&self) -> Vec<ActivityEvent> {
        self.inner
            .read()
            .await
            .events
            .iter()
            .take(20)
            .cloned()
            .collect()
    }
}

impl StoreState {
    fn seeded() -> Self {
        let now = Utc::now();
        Self {
            deposits: vec![
                Deposit {
                    id: Uuid::new_v4(),
                    lp_name: "Atlas LP".to_string(),
                    asset: "USDC".to_string(),
                    amount: 80_000,
                    created_at: now,
                },
                Deposit {
                    id: Uuid::new_v4(),
                    lp_name: "Northstar Capital".to_string(),
                    asset: "USDC".to_string(),
                    amount: 45_000,
                    created_at: now,
                },
            ],
            liquidity_requests: vec![LiquidityRequest {
                id: Uuid::new_v4(),
                merchant_name: "Kairo Market".to_string(),
                asset: "USDC".to_string(),
                amount: 25_000,
                duration_days: 30,
                lease_fee: lease_fee(25_000, 30),
                status: LiquidityStatus::Deployed,
                channel_id: Some("fiber-channel-demo1".to_string()),
                created_at: now,
                updated_at: now,
            }],
            events: vec![
                ActivityEvent {
                    id: Uuid::new_v4(),
                    label: "Lease fee distributed to LP vault".to_string(),
                    amount: Some(250),
                    asset: Some("USDC".to_string()),
                    created_at: now,
                },
                ActivityEvent {
                    id: Uuid::new_v4(),
                    label: "Deployed channel capacity for Kairo Market".to_string(),
                    amount: Some(25_000),
                    asset: Some("USDC".to_string()),
                    created_at: now,
                },
            ],
        }
    }

    fn vault_summary(&self, asset: String) -> VaultSummary {
        let total_deposits = self
            .deposits
            .iter()
            .filter(|deposit| deposit.asset == asset)
            .map(|deposit| deposit.amount)
            .sum();
        let deployed_liquidity = self
            .liquidity_requests
            .iter()
            .filter(|request| request.asset == asset && request.status == LiquidityStatus::Deployed)
            .map(|request| request.amount)
            .sum();
        let fees_earned = self
            .liquidity_requests
            .iter()
            .filter(|request| request.asset == asset && request.status == LiquidityStatus::Deployed)
            .map(|request| request.lease_fee)
            .sum();
        let lp_count = self
            .deposits
            .iter()
            .filter(|deposit| deposit.asset == asset)
            .map(|deposit| deposit.lp_name.as_str())
            .collect::<HashSet<_>>()
            .len();
        let active_requests = self
            .liquidity_requests
            .iter()
            .filter(|request| {
                request.asset == asset && request.status == LiquidityStatus::Requested
            })
            .count();

        VaultSummary {
            asset,
            total_deposits,
            deployed_liquidity,
            available_liquidity: total_deposits - deployed_liquidity,
            fees_earned,
            lp_count,
            active_requests,
        }
    }
}

fn normalize_asset(asset: &str) -> String {
    asset.trim().to_uppercase()
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

fn lease_fee(amount: u64, duration_days: u16) -> u64 {
    let duration_multiplier = u64::from(duration_days).max(1);
    ((amount * duration_multiplier) / 3_000).max(1)
}

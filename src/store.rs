use std::{collections::HashSet, path::PathBuf};

use anyhow::{Result, anyhow};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::{
    domain::{
        ActivityEvent, AuthChallenge, AuthResponse, CapacityReservation, ChallengeRequest,
        ChallengeResponse, CkbScript, ConnectWalletRequest, CreateDepositRequest,
        CreateFeeClaimRequest, CreateLiquidityRequest, CreateSupplyIntentRequest,
        CreateWithdrawalIntentRequest, Dashboard, Deposit, FeeClaim, IntentStatus, LiquidityQuote,
        LiquidityRequest, LiquidityStatus, LpPosition, PositionStatus, ReservationStatus,
        SettleWithdrawalRequest, SupplyIntent, User, UserProfile, UserRole, VaultConfig,
        VaultSummary, VerifyWalletRequest, WithdrawalIntent,
    },
    fiber::FiberClient,
};

pub struct AppStore {
    path: PathBuf,
    fiber: FiberClient,
    vault: VaultConfig,
    inner: RwLock<StoreState>,
}

#[derive(Default, Serialize, Deserialize)]
struct StoreState {
    users: Vec<User>,
    challenges: Vec<AuthChallenge>,
    deposits: Vec<Deposit>,
    #[serde(default)]
    supply_intents: Vec<SupplyIntent>,
    #[serde(default)]
    lp_positions: Vec<LpPosition>,
    #[serde(default)]
    withdrawal_intents: Vec<WithdrawalIntent>,
    #[serde(default)]
    fee_claims: Vec<FeeClaim>,
    #[serde(default)]
    capacity_reservations: Vec<CapacityReservation>,
    liquidity_requests: Vec<LiquidityRequest>,
    events: Vec<ActivityEvent>,
}

impl AppStore {
    pub async fn load(path: PathBuf, fiber: FiberClient, vault: VaultConfig) -> Result<Self> {
        let state = match tokio::fs::read_to_string(&path).await {
            Ok(contents) => serde_json::from_str(&contents)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => StoreState::default(),
            Err(error) => return Err(error.into()),
        };

        Ok(Self {
            path,
            fiber,
            vault,
            inner: RwLock::new(state),
        })
    }

    #[cfg(test)]
    pub fn memory() -> Self {
        Self {
            path: std::env::temp_dir().join(format!("liquidlane-test-{}.json", Uuid::new_v4())),
            fiber: FiberClient::disabled(),
            vault: VaultConfig {
                asset: "CKB".to_string(),
                address: Some("ckt1qpkp7liquidlanevault000000000000000000000000000".to_string()),
                network: "testnet".to_string(),
                configured: true,
                scripts: crate::domain::VaultScripts {
                    vault_lock_code_hash: None,
                    vault_type_code_hash: None,
                    lp_receipt_type_code_hash: None,
                    request_type_code_hash: None,
                    fee_claim_type_code_hash: None,
                },
            },
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

    pub async fn connect_wallet(&self, request: ConnectWalletRequest) -> Result<AuthResponse> {
        let ckb_address = normalize_ckb_address(&request.ckb_address)?;
        let wallet_type = normalize_wallet_type(&request.wallet_type)?;
        if let Some(script) = request.lock_script.as_ref() {
            validate_script(script)?;
        }

        let now = Utc::now();
        let display_name = request
            .display_name
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| short_ckb_address(&ckb_address));
        let mut state = self.inner.write().await;
        let user_index = state
            .users
            .iter()
            .position(|user| user.ckb_address == ckb_address && user.wallet_type == wallet_type);

        let user = match user_index {
            Some(index) => {
                state.users[index].display_name = display_name.trim().to_string();
                state.users[index].role = request.role;
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
                label: format!("{} opened a wallet session", user.display_name),
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
            .map(|asset| asset.trim().to_uppercase())
            .filter(|asset| !asset.is_empty())
            .unwrap_or_else(|| self.vault.asset.clone());
        Dashboard {
            user: UserProfile::from(user),
            vault: state.vault_summary(&self.vault, asset),
            deposits: state.visible_deposits(user),
            positions: state.visible_positions(user),
            liquidity_requests: state.visible_liquidity_requests(user),
            reservations: state.visible_reservations(user),
            withdrawals: state.visible_withdrawals(user),
            fee_claims: state.visible_fee_claims(user),
            activity: state.visible_activity(user),
        }
    }

    pub async fn create_supply_intent(
        &self,
        user: &User,
        request: CreateSupplyIntentRequest,
    ) -> Result<SupplyIntent> {
        require_role(user, &[UserRole::Lp, UserRole::Operator])?;
        ensure_vault_configured(&self.vault)?;
        validate_amount(request.amount)?;
        validate_required("asset", &request.asset)?;
        let asset = normalize_asset(&request.asset);
        if asset != self.vault.asset {
            return Err(anyhow!(
                "supply asset must match the active {} vault",
                self.vault.asset
            ));
        }

        let now = Utc::now();
        let id = Uuid::new_v4();
        let intent = SupplyIntent {
            id,
            lp_id: user.id,
            lp_name: user.display_name.clone(),
            ckb_address: user.ckb_address.clone(),
            asset,
            amount: request.amount,
            vault_address: self.vault.address.clone().expect("vault configured"),
            receipt_cell_id: receipt_cell_id(id),
            memo: format!("LL_SUPPLY:{id}:{}:{}", self.vault.asset, request.amount),
            status: IntentStatus::PendingSignature,
            tx_hash: None,
            created_at: now,
            expires_at: now + Duration::minutes(15),
        };

        let mut state = self.inner.write().await;
        state.supply_intents.push(intent.clone());
        self.persist_locked(&state).await?;
        Ok(intent)
    }

    pub async fn create_withdrawal_intent(
        &self,
        user: &User,
        request: CreateWithdrawalIntentRequest,
    ) -> Result<WithdrawalIntent> {
        require_role(user, &[UserRole::Lp, UserRole::Operator])?;
        validate_amount(request.amount)?;
        let mut state = self.inner.write().await;
        let position = state
            .lp_positions
            .iter()
            .find(|position| position.id == request.position_id)
            .ok_or_else(|| anyhow!("LP position not found"))?;
        if user.role != UserRole::Operator && position.lp_id != user.id {
            return Err(anyhow!("you can only withdraw your own LP position"));
        }
        if position.status != PositionStatus::Active {
            return Err(anyhow!("LP position is not active"));
        }
        if position.available_amount < request.amount {
            return Err(anyhow!(
                "only {} {} is available to withdraw",
                position.available_amount,
                position.asset
            ));
        }

        let now = Utc::now();
        let id = Uuid::new_v4();
        let intent = WithdrawalIntent {
            id,
            lp_id: position.lp_id,
            lp_name: position.lp_name.clone(),
            ckb_address: position.ckb_address.clone(),
            position_id: position.id,
            asset: position.asset.clone(),
            amount: request.amount,
            receipt_cell_id: position.receipt_cell_id.clone(),
            memo: format!("LL_WITHDRAW:{id}:{}:{}", position.asset, request.amount),
            status: IntentStatus::PendingSignature,
            tx_hash: None,
            created_at: now,
            expires_at: now + Duration::minutes(15),
        };
        state.withdrawal_intents.push(intent.clone());
        self.persist_locked(&state).await?;
        Ok(intent)
    }

    pub async fn settle_withdrawal(
        &self,
        user: &User,
        id: Uuid,
        request: SettleWithdrawalRequest,
    ) -> Result<WithdrawalIntent> {
        require_role(user, &[UserRole::Lp, UserRole::Operator])?;
        validate_transaction_proof(&request.tx_hash, &request.signed_tx)?;
        let tx_hash = normalize_transaction_hash(&request.tx_hash, &request.signed_tx)
            .ok_or_else(|| anyhow!("withdrawal settlement requires tx_hash"))?;

        let mut state = self.inner.write().await;
        let intent_index = state
            .withdrawal_intents
            .iter()
            .position(|intent| intent.id == id)
            .ok_or_else(|| anyhow!("withdrawal intent not found"))?;
        let intent = state.withdrawal_intents[intent_index].clone();
        if user.role != UserRole::Operator && intent.lp_id != user.id {
            return Err(anyhow!("you can only settle your own withdrawal"));
        }
        validate_pending_intent(&intent.status, intent.expires_at)?;

        let position = state
            .lp_positions
            .iter_mut()
            .find(|position| position.id == intent.position_id)
            .ok_or_else(|| anyhow!("LP position not found"))?;
        if position.available_amount < intent.amount {
            return Err(anyhow!(
                "withdrawal intent exceeds available position balance"
            ));
        }
        position.available_amount -= intent.amount;
        position.supplied_amount -= intent.amount;
        position.updated_at = Utc::now();
        if position.supplied_amount == 0 {
            position.status = PositionStatus::Closed;
        }

        state.withdrawal_intents[intent_index].status = IntentStatus::Settled;
        state.withdrawal_intents[intent_index].tx_hash = Some(tx_hash.clone());
        let settled = state.withdrawal_intents[intent_index].clone();
        state.events.insert(
            0,
            ActivityEvent {
                id: Uuid::new_v4(),
                actor_id: user.id,
                label: format!(
                    "{} withdrew liquidity from the {} vault",
                    user.display_name, settled.asset
                ),
                amount: Some(settled.amount),
                asset: Some(settled.asset.clone()),
                created_at: Utc::now(),
            },
        );
        self.persist_locked(&state).await?;
        Ok(settled)
    }

    pub async fn create_fee_claim(
        &self,
        user: &User,
        request: CreateFeeClaimRequest,
    ) -> Result<FeeClaim> {
        require_role(user, &[UserRole::Lp, UserRole::Operator])?;
        validate_amount(request.amount)?;
        let mut state = self.inner.write().await;
        let position = state
            .lp_positions
            .iter()
            .find(|position| position.id == request.position_id)
            .ok_or_else(|| anyhow!("LP position not found"))?;
        if user.role != UserRole::Operator && position.lp_id != user.id {
            return Err(anyhow!("you can only claim fees for your own LP position"));
        }
        let claimable = position.fees_earned.saturating_sub(position.fees_claimed);
        if claimable < request.amount {
            return Err(anyhow!(
                "only {} {} is claimable",
                claimable,
                position.asset
            ));
        }

        let now = Utc::now();
        let id = Uuid::new_v4();
        let claim = FeeClaim {
            id,
            lp_id: position.lp_id,
            position_id: position.id,
            asset: position.asset.clone(),
            amount: request.amount,
            memo: format!("LL_FEE_CLAIM:{id}:{}:{}", position.asset, request.amount),
            status: IntentStatus::PendingSignature,
            tx_hash: None,
            created_at: now,
            expires_at: now + Duration::minutes(15),
        };
        state.fee_claims.push(claim.clone());
        self.persist_locked(&state).await?;
        Ok(claim)
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
            .vault_summary(&self.vault, asset.clone())
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
        let asset = normalize_asset(&request.asset);
        if !self.vault.configured {
            return Err(anyhow!("active vault address is not configured"));
        }
        if asset != self.vault.asset {
            return Err(anyhow!(
                "supply asset must match the active {} vault",
                self.vault.asset
            ));
        }
        validate_deposit_transaction(&request)?;
        let tx_hash = normalize_deposit_tx_hash(&request)
            .ok_or_else(|| anyhow!("supply settlement requires tx_hash"))?;
        let intent_id = request
            .intent_id
            .ok_or_else(|| anyhow!("supply settlement requires intent_id"))?;

        let mut state = self.inner.write().await;
        let intent_index = state
            .supply_intents
            .iter()
            .position(|intent| intent.id == intent_id)
            .ok_or_else(|| anyhow!("supply intent not found"))?;
        let intent = state.supply_intents[intent_index].clone();
        if user.role != UserRole::Operator && intent.lp_id != user.id {
            return Err(anyhow!("you can only settle your own supply intent"));
        }
        validate_pending_intent(&intent.status, intent.expires_at)?;
        if intent.asset != asset || intent.amount != request.amount {
            return Err(anyhow!("supply settlement does not match the intent"));
        }

        let now = Utc::now();
        let deposit = Deposit {
            id: Uuid::new_v4(),
            lp_id: user.id,
            lp_name: user.display_name.clone(),
            ckb_address: user.ckb_address.clone(),
            asset,
            amount: request.amount,
            tx_hash: Some(tx_hash.clone()),
            signed_tx: request.signed_tx,
            created_at: now,
        };
        let position = LpPosition {
            id: Uuid::new_v4(),
            lp_id: user.id,
            lp_name: user.display_name.clone(),
            ckb_address: user.ckb_address.clone(),
            asset: deposit.asset.clone(),
            supplied_amount: deposit.amount,
            available_amount: deposit.amount,
            reserved_amount: 0,
            deployed_amount: 0,
            fees_earned: 0,
            fees_claimed: 0,
            receipt_cell_id: intent.receipt_cell_id.clone(),
            supply_tx_hash: tx_hash.clone(),
            status: PositionStatus::Active,
            created_at: now,
            updated_at: now,
        };

        state.supply_intents[intent_index].status = IntentStatus::Settled;
        state.supply_intents[intent_index].tx_hash = Some(tx_hash);
        state.events.insert(
            0,
            ActivityEvent {
                id: Uuid::new_v4(),
                actor_id: user.id,
                label: format!(
                    "{} supplied liquidity to the {} vault",
                    user.display_name, deposit.asset
                ),
                amount: Some(deposit.amount),
                asset: Some(deposit.asset.clone()),
                created_at: deposit.created_at,
            },
        );
        state.lp_positions.push(position);
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
            .vault_summary(&self.vault, liquidity_request.asset.clone())
            .available_liquidity
            < liquidity_request.amount
        {
            return Err(anyhow!("liquidity was just reserved by another request"));
        }
        reserve_positions(
            &mut state.lp_positions,
            &liquidity_request.asset,
            liquidity_request.amount,
            now,
        )?;
        let reservation = CapacityReservation {
            id: Uuid::new_v4(),
            request_id: liquidity_request.id,
            merchant_id: user.id,
            merchant_name: user.display_name.clone(),
            ckb_address: user.ckb_address.clone(),
            asset: liquidity_request.asset.clone(),
            amount: liquidity_request.amount,
            lease_fee: liquidity_request.lease_fee,
            request_cell_id: request_cell_id(liquidity_request.id),
            status: ReservationStatus::Reserved,
            created_at: now,
            updated_at: now,
        };
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
        state.capacity_reservations.push(reservation);
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
        if let Some(reservation) = state
            .capacity_reservations
            .iter_mut()
            .find(|reservation| reservation.request_id == updated.id)
        {
            reservation.updated_at = now;
            match updated.status {
                LiquidityStatus::PendingFiberChannel | LiquidityStatus::ChannelOpen => {
                    reservation.status = ReservationStatus::Deployed;
                    deploy_reserved_positions(
                        &mut state.lp_positions,
                        &updated.asset,
                        updated.amount,
                        updated.lease_fee,
                        now,
                    )?;
                    state.events.insert(
                        0,
                        ActivityEvent {
                            id: Uuid::new_v4(),
                            actor_id: user.id,
                            label: "Lease fee distributed to LP positions".to_string(),
                            amount: Some(updated.lease_fee),
                            asset: Some(updated.asset.clone()),
                            created_at: now,
                        },
                    );
                }
                LiquidityStatus::Failed => {
                    reservation.status = ReservationStatus::Failed;
                    release_reserved_positions(
                        &mut state.lp_positions,
                        &updated.asset,
                        updated.amount,
                        now,
                    )?;
                }
                LiquidityStatus::Requested => {}
            }
        }
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
    fn vault_summary(&self, vault: &VaultConfig, asset: String) -> VaultSummary {
        let active_positions = self.lp_positions.iter().filter(|position| {
            position.asset == asset && position.status == PositionStatus::Active
        });
        let total_deposits = active_positions
            .clone()
            .map(|position| position.supplied_amount)
            .sum::<u64>();
        let available_liquidity = self
            .lp_positions
            .iter()
            .filter(|position| position.asset == asset && position.status == PositionStatus::Active)
            .map(|position| position.available_amount)
            .sum::<u64>();
        let reserved_liquidity = self
            .lp_positions
            .iter()
            .filter(|position| position.asset == asset && position.status == PositionStatus::Active)
            .map(|position| position.reserved_amount)
            .sum::<u64>();
        let deployed_liquidity = self
            .lp_positions
            .iter()
            .filter(|position| position.asset == asset && position.status == PositionStatus::Active)
            .map(|position| position.deployed_amount)
            .sum::<u64>();
        let pending_channel_liquidity = self
            .liquidity_requests
            .iter()
            .filter(|request| {
                request.asset == asset && request.status == LiquidityStatus::PendingFiberChannel
            })
            .map(|request| request.amount)
            .sum::<u64>();
        let fees_earned = self
            .lp_positions
            .iter()
            .filter(|position| position.asset == asset && position.status == PositionStatus::Active)
            .map(|position| position.fees_earned)
            .sum::<u64>();
        let lp_count = self
            .lp_positions
            .iter()
            .filter(|position| position.asset == asset && position.status == PositionStatus::Active)
            .map(|position| position.lp_id)
            .collect::<HashSet<_>>()
            .len();
        let active_requests = self
            .capacity_reservations
            .iter()
            .filter(|reservation| {
                reservation.asset == asset
                    && matches!(
                        reservation.status,
                        ReservationStatus::Reserved | ReservationStatus::Deployed
                    )
            })
            .count();

        VaultSummary {
            asset,
            address: vault.address.clone(),
            network: vault.network.clone(),
            configured: vault.configured,
            total_deposits,
            reserved_liquidity,
            pending_channel_liquidity,
            deployed_liquidity,
            available_liquidity,
            fees_earned,
            lp_count,
            active_requests,
        }
    }

    fn visible_deposits(&self, user: &User) -> Vec<Deposit> {
        match user.role {
            UserRole::Operator | UserRole::Merchant => self
                .deposits
                .iter()
                .filter(|deposit| is_verified_deposit(deposit))
                .cloned()
                .collect(),
            UserRole::Lp => self
                .deposits
                .iter()
                .filter(|deposit| deposit.lp_id == user.id && is_verified_deposit(deposit))
                .cloned()
                .collect(),
        }
    }

    fn visible_positions(&self, user: &User) -> Vec<LpPosition> {
        match user.role {
            UserRole::Operator | UserRole::Merchant => self.lp_positions.clone(),
            UserRole::Lp => self
                .lp_positions
                .iter()
                .filter(|position| position.lp_id == user.id)
                .cloned()
                .collect(),
        }
    }

    fn visible_reservations(&self, user: &User) -> Vec<CapacityReservation> {
        match user.role {
            UserRole::Operator | UserRole::Lp => self.capacity_reservations.clone(),
            UserRole::Merchant => self
                .capacity_reservations
                .iter()
                .filter(|reservation| reservation.merchant_id == user.id)
                .cloned()
                .collect(),
        }
    }

    fn visible_withdrawals(&self, user: &User) -> Vec<WithdrawalIntent> {
        match user.role {
            UserRole::Operator => self.withdrawal_intents.clone(),
            _ => self
                .withdrawal_intents
                .iter()
                .filter(|intent| intent.lp_id == user.id)
                .cloned()
                .collect(),
        }
    }

    fn visible_fee_claims(&self, user: &User) -> Vec<FeeClaim> {
        match user.role {
            UserRole::Operator => self.fee_claims.clone(),
            _ => self
                .fee_claims
                .iter()
                .filter(|claim| claim.lp_id == user.id)
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
            .filter(|event| is_product_activity(event))
            .filter(|event| {
                user.role == UserRole::Operator
                    || event.actor_id == user.id
                    || event.label.contains("Lease fee")
            })
            .take(15)
            .cloned()
            .collect()
    }
}

fn ensure_vault_configured(vault: &VaultConfig) -> Result<()> {
    if !vault.configured
        || vault
            .address
            .as_deref()
            .unwrap_or_default()
            .trim()
            .is_empty()
    {
        return Err(anyhow!("active vault address is not configured"));
    }
    Ok(())
}

fn validate_pending_intent(status: &IntentStatus, expires_at: chrono::DateTime<Utc>) -> Result<()> {
    if status != &IntentStatus::PendingSignature {
        return Err(anyhow!("intent is not pending signature"));
    }
    if expires_at < Utc::now() {
        return Err(anyhow!("intent has expired"));
    }
    Ok(())
}

fn receipt_cell_id(id: Uuid) -> String {
    format!("ll-receipt-{id}")
}

fn request_cell_id(id: Uuid) -> String {
    format!("ll-request-{id}")
}

fn reserve_positions(
    positions: &mut [LpPosition],
    asset: &str,
    mut amount: u64,
    now: chrono::DateTime<Utc>,
) -> Result<()> {
    for position in positions
        .iter_mut()
        .filter(|position| position.asset == asset && position.status == PositionStatus::Active)
    {
        if amount == 0 {
            break;
        }
        let taken = position.available_amount.min(amount);
        if taken == 0 {
            continue;
        }
        position.available_amount -= taken;
        position.reserved_amount += taken;
        position.updated_at = now;
        amount -= taken;
    }
    if amount > 0 {
        return Err(anyhow!("liquidity was just reserved by another request"));
    }
    Ok(())
}

fn deploy_reserved_positions(
    positions: &mut [LpPosition],
    asset: &str,
    mut amount: u64,
    lease_fee: u64,
    now: chrono::DateTime<Utc>,
) -> Result<()> {
    let total_amount = amount.max(1);
    let mut undistributed_fee = lease_fee;

    for position in positions
        .iter_mut()
        .filter(|position| position.asset == asset && position.status == PositionStatus::Active)
    {
        if amount == 0 {
            break;
        }
        let moved = position.reserved_amount.min(amount);
        if moved == 0 {
            continue;
        }
        let fee_share = if amount == moved {
            undistributed_fee
        } else {
            lease_fee
                .saturating_mul(moved)
                .saturating_div(total_amount)
                .min(undistributed_fee)
        };

        position.reserved_amount -= moved;
        position.deployed_amount += moved;
        position.fees_earned += fee_share;
        position.updated_at = now;
        amount -= moved;
        undistributed_fee = undistributed_fee.saturating_sub(fee_share);
    }
    if amount > 0 {
        return Err(anyhow!("reserved liquidity accounting is incomplete"));
    }
    Ok(())
}

fn release_reserved_positions(
    positions: &mut [LpPosition],
    asset: &str,
    mut amount: u64,
    now: chrono::DateTime<Utc>,
) -> Result<()> {
    for position in positions
        .iter_mut()
        .filter(|position| position.asset == asset && position.status == PositionStatus::Active)
    {
        if amount == 0 {
            break;
        }
        let released = position.reserved_amount.min(amount);
        if released == 0 {
            continue;
        }
        position.reserved_amount -= released;
        position.available_amount += released;
        position.updated_at = now;
        amount -= released;
    }
    if amount > 0 {
        return Err(anyhow!("reserved liquidity accounting is incomplete"));
    }
    Ok(())
}

fn validate_transaction_proof(
    tx_hash: &Option<String>,
    signed_tx: &Option<serde_json::Value>,
) -> Result<()> {
    if let Some(tx_hash) = normalize_transaction_hash(tx_hash, signed_tx).as_deref() {
        validate_tx_hash(tx_hash)?;
    }
    let signed_tx = signed_tx
        .as_ref()
        .ok_or_else(|| anyhow!("signed CKB transaction proof is required"))?;
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

fn normalize_transaction_hash(
    tx_hash: &Option<String>,
    signed_tx: &Option<serde_json::Value>,
) -> Option<String> {
    normalize_optional(tx_hash).or_else(|| {
        signed_tx
            .as_ref()
            .and_then(|tx| tx.get("hash"))
            .and_then(|hash| hash.as_str())
            .map(str::trim)
            .filter(|hash| !hash.is_empty())
            .map(str::to_string)
    })
}

fn is_verified_deposit(deposit: &Deposit) -> bool {
    deposit.signed_tx.is_some() && deposit.tx_hash.is_some()
}

fn is_product_activity(event: &ActivityEvent) -> bool {
    (event.amount.is_some() && !event.label.contains("deposited vault liquidity"))
        || event.label.contains("Fiber")
        || event.label.contains("reserved")
        || event.label.contains("supplied")
        || event.label.contains("Lease fee")
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

fn validate_deposit_transaction(request: &CreateDepositRequest) -> Result<()> {
    let tx_hash = normalize_deposit_tx_hash(request);
    if let Some(tx_hash) = tx_hash.as_deref() {
        validate_tx_hash(tx_hash)?;
    }

    let signed_tx = request
        .signed_tx
        .as_ref()
        .ok_or_else(|| anyhow!("supply liquidity requires a signed CKB transaction proof"))?;
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
    let witnesses = signed_tx
        .get("witnesses")
        .and_then(|value| value.as_array())
        .ok_or_else(|| anyhow!("signed_tx.witnesses must be an array"))?;
    if witnesses.is_empty() {
        return Err(anyhow!("signed_tx must include at least one witness"));
    }
    Ok(())
}

fn normalize_deposit_tx_hash(request: &CreateDepositRequest) -> Option<String> {
    normalize_optional(&request.tx_hash).or_else(|| {
        request
            .signed_tx
            .as_ref()
            .and_then(|tx| tx.get("hash"))
            .and_then(|hash| hash.as_str())
            .map(str::trim)
            .filter(|hash| !hash.is_empty())
            .map(str::to_string)
    })
}

fn validate_tx_hash(tx_hash: &str) -> Result<()> {
    validate_hex_field("tx_hash", tx_hash, 66)
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

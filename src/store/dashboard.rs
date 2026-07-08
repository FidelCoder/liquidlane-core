use std::collections::HashSet;

use super::{
    AppStore, StoreState,
    accounting::request_cell_id,
    validation::{is_product_activity, is_verified_deposit},
};
use crate::domain::{
    ActivityEvent, CapacityReservation, Dashboard, Deposit, FeeClaim, LiquidityRequest,
    LiquidityStatus, LpPosition, PositionStatus, ReservationStatus, User, UserProfile, UserRole,
    VaultConfig, VaultSummary, WithdrawalIntent,
};

impl AppStore {
    pub async fn dashboard(&self, user: &User, asset: Option<String>) -> Dashboard {
        let vault = self.vault_config().await;
        let asset = asset
            .map(|asset| asset.trim().to_uppercase())
            .filter(|asset| !asset.is_empty())
            .unwrap_or_else(|| vault.asset.clone());
        if let Err(error) = self.sync_user_lp_receipts(user, &asset, &vault).await {
            tracing::warn!(error = %error, "failed to sync LP receipt cells from CKB");
        }
        let state = self.inner.read().await;
        Dashboard {
            user: UserProfile::from(user),
            vault: state.vault_summary(&vault, asset),
            deposits: state.visible_deposits(user),
            positions: state.visible_positions(user),
            liquidity_requests: state.visible_liquidity_requests(user),
            reservations: state.visible_reservations(user),
            withdrawals: state.visible_withdrawals(user),
            fee_claims: state.visible_fee_claims(user),
            activity: state.visible_activity(user),
        }
    }
}

impl StoreState {
    pub(super) fn vault_summary(&self, vault: &VaultConfig, asset: String) -> VaultSummary {
        let active = |position: &&LpPosition| {
            position.asset == asset && position.status == PositionStatus::Active
        };
        let total_deposits = self
            .lp_positions
            .iter()
            .filter(active)
            .map(|position| position.supplied_amount)
            .sum::<u64>();
        let available_liquidity = self
            .lp_positions
            .iter()
            .filter(active)
            .map(|position| position.available_amount)
            .sum::<u64>();
        let reserved_liquidity = self
            .lp_positions
            .iter()
            .filter(active)
            .map(|position| position.reserved_amount)
            .sum::<u64>();
        let deployed_liquidity = self
            .lp_positions
            .iter()
            .filter(active)
            .map(|position| position.deployed_amount)
            .sum::<u64>();
        let fees_earned = self
            .lp_positions
            .iter()
            .filter(active)
            .map(|position| position.fees_earned)
            .sum::<u64>();
        let pending_channel_liquidity = self
            .liquidity_requests
            .iter()
            .filter(|request| {
                request.asset == asset && request.status == LiquidityStatus::PendingFiberChannel
            })
            .map(|request| request.amount)
            .sum::<u64>();
        let lp_count = self
            .lp_positions
            .iter()
            .filter(active)
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
            cell_out_point: vault.cell_out_point.clone(),
            network: vault.network.clone(),
            configured: vault.configured,
            scripts: vault.scripts.clone(),
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
        let positions = match user.role {
            UserRole::Operator | UserRole::Merchant => self.lp_positions.clone(),
            UserRole::Lp => self
                .lp_positions
                .iter()
                .filter(|position| position.lp_id == user.id)
                .cloned()
                .collect(),
        };
        positions.into_iter().map(normalize_position_view).collect()
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
        let requests = match user.role {
            UserRole::Operator | UserRole::Lp => self.liquidity_requests.clone(),
            UserRole::Merchant => self
                .liquidity_requests
                .iter()
                .filter(|request| request.merchant_id == user.id)
                .cloned()
                .collect(),
        };
        requests.into_iter().map(normalize_request_view).collect()
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

fn normalize_request_view(mut request: LiquidityRequest) -> LiquidityRequest {
    if request.request_cell_id.trim().is_empty() {
        request.request_cell_id = request_cell_id(request.id);
    }
    request
}

fn normalize_position_view(mut position: LpPosition) -> LpPosition {
    if position.receipt_cell_out_point.is_none() {
        position.receipt_cell_out_point = Some(format!("{}#0x1", position.supply_tx_hash));
    }
    position
}

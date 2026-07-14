use anyhow::{Result, anyhow};
use chrono::{Duration, Utc};
use uuid::Uuid;

use super::{
    AppStore,
    accounting::receipt_cell_id,
    validation::{
        ensure_vault_configured, normalize_asset, normalize_deposit_tx_hash, require_role,
        validate_amount, validate_deposit_transaction, validate_pending_intent, validate_required,
    },
    vault_output_out_point,
};
use crate::domain::{
    ActivityEvent, CreateDepositRequest, CreateSupplyIntentRequest, Deposit, IntentStatus,
    LpPosition, PositionStatus, SupplyIntent, User, UserRole,
};

impl AppStore {
    pub async fn create_supply_intent(
        &self,
        user: &User,
        request: CreateSupplyIntentRequest,
    ) -> Result<SupplyIntent> {
        require_role(user, &[UserRole::Lp, UserRole::Operator])?;
        let vault = self.vault_config().await;
        ensure_vault_configured(&vault)?;
        validate_amount(request.amount)?;
        validate_required("asset", &request.asset)?;
        let asset = normalize_asset(&request.asset);
        if asset != vault.asset {
            return Err(anyhow!(
                "supply asset must match the active {} vault",
                vault.asset
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
            vault_address: vault.address.clone().expect("vault configured"),
            receipt_cell_id: receipt_cell_id(id),
            memo: format!("LL_SUPPLY:{id}:{}:{}", vault.asset, request.amount),
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

    pub async fn create_deposit(
        &self,
        user: &User,
        request: CreateDepositRequest,
    ) -> Result<Deposit> {
        require_role(user, &[UserRole::Lp, UserRole::Operator])?;
        validate_amount(request.amount)?;
        validate_required("asset", &request.asset)?;
        let asset = normalize_asset(&request.asset);
        let vault = self.vault_config().await;
        ensure_vault_configured(&vault)?;
        if asset != vault.asset {
            return Err(anyhow!(
                "supply asset must match the active {} vault",
                vault.asset
            ));
        }
        validate_deposit_transaction(&request)?;
        let tx_hash = normalize_deposit_tx_hash(&request)
            .ok_or_else(|| anyhow!("supply settlement requires tx_hash"))?;
        let intent_id = request
            .intent_id
            .ok_or_else(|| anyhow!("supply settlement requires intent_id"))?;

        let intent = {
            let state = self.inner.read().await;
            let intent = state
                .supply_intents
                .iter()
                .find(|intent| intent.id == intent_id)
                .cloned()
                .ok_or_else(|| anyhow!("supply intent not found"))?;
            if user.role != UserRole::Operator && intent.lp_id != user.id {
                return Err(anyhow!("you can only settle your own supply intent"));
            }
            validate_pending_intent(&intent.status, intent.expires_at)?;
            if intent.asset != asset || intent.amount != request.amount {
                return Err(anyhow!("supply settlement does not match the intent"));
            }
            intent
        };
        self.verify_vault_deposit_tx(&tx_hash, &intent, user, &request.signed_tx)
            .await?;

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
        let position = lp_position(user, &deposit, &intent.receipt_cell_id, &tx_hash, now);

        let mut state = self.inner.write().await;
        let intent_index = state
            .supply_intents
            .iter()
            .position(|stored| stored.id == intent_id)
            .ok_or_else(|| anyhow!("supply intent not found"))?;
        validate_pending_intent(
            &state.supply_intents[intent_index].status,
            state.supply_intents[intent_index].expires_at,
        )?;
        state.supply_intents[intent_index].status = IntentStatus::Settled;
        state.supply_intents[intent_index].tx_hash = Some(tx_hash.clone());
        state.vault_cell_out_point = Some(vault_output_out_point(&tx_hash));
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
}

fn lp_position(
    user: &User,
    deposit: &Deposit,
    receipt_cell_id: &str,
    tx_hash: &str,
    now: chrono::DateTime<Utc>,
) -> LpPosition {
    LpPosition {
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
        receipt_cell_id: receipt_cell_id.to_string(),
        receipt_cell_out_point: Some(format!("{tx_hash}#0x1")),
        supply_tx_hash: tx_hash.to_string(),
        status: PositionStatus::Active,
        created_at: now,
        updated_at: now,
    }
}

use anyhow::{Result, anyhow};
use chrono::{Duration, Utc};
use uuid::Uuid;

use super::{
    AppStore,
    validation::{
        normalize_optional, normalize_transaction_hash, require_role, validate_amount,
        validate_pending_intent, validate_transaction_proof,
    },
};
use crate::domain::{
    ActivityEvent, CreateFeeClaimRequest, CreateWithdrawalIntentRequest, FeeClaim, IntentStatus,
    LpPosition, PositionStatus, SettleFeeClaimRequest, SettleWithdrawalRequest, User, UserRole,
    WithdrawalIntent,
};

impl AppStore {
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
        validate_position_owner(user, position)?;
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
        let (intent, position) = self.withdrawal_context(user, id).await?;
        self.verify_withdrawal_tx(&tx_hash, &intent, &position, user, &request.signed_tx)
            .await?;

        let mut state = self.inner.write().await;
        settle_position_withdrawal(
            &mut state.lp_positions,
            &intent,
            normalize_optional(&request.receipt_cell_out_point),
        )?;
        let intent_index = state
            .withdrawal_intents
            .iter()
            .position(|intent| intent.id == id)
            .ok_or_else(|| anyhow!("withdrawal intent not found"))?;
        state.withdrawal_intents[intent_index].status = IntentStatus::Settled;
        state.withdrawal_intents[intent_index].tx_hash = Some(tx_hash);
        let settled = state.withdrawal_intents[intent_index].clone();
        state.events.insert(0, withdrawal_event(user, &settled));
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
        validate_position_owner(user, position)?;
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

    pub async fn settle_fee_claim(
        &self,
        user: &User,
        id: Uuid,
        request: SettleFeeClaimRequest,
    ) -> Result<FeeClaim> {
        require_role(user, &[UserRole::Lp, UserRole::Operator])?;
        validate_transaction_proof(&request.tx_hash, &request.signed_tx)?;
        let tx_hash = normalize_transaction_hash(&request.tx_hash, &request.signed_tx)
            .ok_or_else(|| anyhow!("fee claim settlement requires tx_hash"))?;
        let (claim, position) = self.fee_claim_context(user, id).await?;
        self.verify_fee_claim_tx(&tx_hash, &claim, &position, user, &request.signed_tx)
            .await?;

        let mut state = self.inner.write().await;
        settle_position_fee_claim(
            &mut state.lp_positions,
            &claim,
            normalize_optional(&request.receipt_cell_out_point),
        )?;
        let claim_index = state
            .fee_claims
            .iter()
            .position(|claim| claim.id == id)
            .ok_or_else(|| anyhow!("fee claim intent not found"))?;
        state.fee_claims[claim_index].status = IntentStatus::Settled;
        state.fee_claims[claim_index].tx_hash = Some(tx_hash);
        let settled = state.fee_claims[claim_index].clone();
        state.events.insert(0, fee_claim_event(user, &settled));
        self.persist_locked(&state).await?;
        Ok(settled)
    }

    async fn withdrawal_context(
        &self,
        user: &User,
        id: Uuid,
    ) -> Result<(WithdrawalIntent, LpPosition)> {
        let state = self.inner.read().await;
        let intent = state
            .withdrawal_intents
            .iter()
            .find(|intent| intent.id == id)
            .ok_or_else(|| anyhow!("withdrawal intent not found"))?;
        if user.role != UserRole::Operator && intent.lp_id != user.id {
            return Err(anyhow!("you can only settle your own withdrawal"));
        }
        validate_pending_intent(&intent.status, intent.expires_at)?;
        let position = state
            .lp_positions
            .iter()
            .find(|position| position.id == intent.position_id)
            .ok_or_else(|| anyhow!("LP position not found"))?;
        Ok((intent.clone(), position.clone()))
    }

    async fn fee_claim_context(&self, user: &User, id: Uuid) -> Result<(FeeClaim, LpPosition)> {
        let state = self.inner.read().await;
        let claim = state
            .fee_claims
            .iter()
            .find(|claim| claim.id == id)
            .ok_or_else(|| anyhow!("fee claim intent not found"))?;
        if user.role != UserRole::Operator && claim.lp_id != user.id {
            return Err(anyhow!("you can only settle your own fee claim"));
        }
        validate_pending_intent(&claim.status, claim.expires_at)?;
        let position = state
            .lp_positions
            .iter()
            .find(|position| position.id == claim.position_id)
            .ok_or_else(|| anyhow!("LP position not found"))?;
        Ok((claim.clone(), position.clone()))
    }
}

fn validate_position_owner(user: &User, position: &LpPosition) -> Result<()> {
    if user.role != UserRole::Operator && position.lp_id != user.id {
        return Err(anyhow!("you can only manage your own LP position"));
    }
    Ok(())
}

fn settle_position_withdrawal(
    positions: &mut [LpPosition],
    intent: &WithdrawalIntent,
    receipt_out_point: Option<String>,
) -> Result<()> {
    let position = position_mut(positions, intent.position_id)?;
    if position.available_amount < intent.amount {
        return Err(anyhow!(
            "withdrawal intent exceeds available position balance"
        ));
    }
    position.available_amount -= intent.amount;
    position.supplied_amount -= intent.amount;
    position.receipt_cell_out_point =
        receipt_out_point.or_else(|| position.receipt_cell_out_point.clone());
    position.updated_at = Utc::now();
    if position.supplied_amount == 0 {
        position.status = PositionStatus::Closed;
        position.receipt_cell_out_point = None;
    }
    Ok(())
}

fn settle_position_fee_claim(
    positions: &mut [LpPosition],
    claim: &FeeClaim,
    receipt_out_point: Option<String>,
) -> Result<()> {
    let position = position_mut(positions, claim.position_id)?;
    let claimable = position.fees_earned.saturating_sub(position.fees_claimed);
    if claimable < claim.amount {
        return Err(anyhow!("fee claim exceeds claimable position balance"));
    }
    position.fees_claimed += claim.amount;
    position.receipt_cell_out_point =
        receipt_out_point.or_else(|| position.receipt_cell_out_point.clone());
    position.updated_at = Utc::now();
    Ok(())
}

fn position_mut(positions: &mut [LpPosition], id: Uuid) -> Result<&mut LpPosition> {
    positions
        .iter_mut()
        .find(|position| position.id == id)
        .ok_or_else(|| anyhow!("LP position not found"))
}

fn withdrawal_event(user: &User, settled: &WithdrawalIntent) -> ActivityEvent {
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
    }
}

fn fee_claim_event(user: &User, settled: &FeeClaim) -> ActivityEvent {
    ActivityEvent {
        id: Uuid::new_v4(),
        actor_id: user.id,
        label: format!("{} claimed LP fees", user.display_name),
        amount: Some(settled.amount),
        asset: Some(settled.asset.clone()),
        created_at: Utc::now(),
    }
}

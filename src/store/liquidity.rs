use anyhow::{Result, anyhow};
use chrono::Utc;
use uuid::Uuid;

use super::{
    AppStore,
    accounting::{request_cell_id, reserve_positions_with_fee},
    validation::{
        lease_fee, normalize_asset, normalize_optional, normalize_transaction_hash, require_role,
        validate_liquidity_request, validate_pending_intent, validate_transaction_proof,
    },
    vault_output_out_point,
};
use crate::domain::{
    ActivityEvent, CapacityReservation, CreateLiquidityRequest, IntentStatus, LiquidityQuote,
    LiquidityRequest, LiquidityStatus, RECEIVER_NODE_RESERVE_MIN_CKB,
    RECEIVER_NODE_RESERVE_RECOMMENDED_CKB, RequestIntent, ReservationStatus, User, UserRole,
};

impl AppStore {
    pub async fn quote(
        &self,
        user: &User,
        request: &CreateLiquidityRequest,
    ) -> Result<LiquidityQuote> {
        require_role(user, &[UserRole::Merchant, UserRole::Operator])?;
        validate_liquidity_request(request)?;

        let asset = normalize_asset(&request.asset);
        let vault = self.vault_config().await;
        if let Err(error) = self.sync_live_vault_accounting(&vault, &asset).await {
            tracing::warn!(error = %error, "failed to sync live vault accounting for quote");
        }
        let available_liquidity = self
            .inner
            .read()
            .await
            .vault_summary(&vault, asset.clone())
            .available_liquidity;

        Ok(LiquidityQuote {
            asset,
            amount: request.amount,
            duration_days: request.duration_days,
            lease_fee: lease_fee(request.amount, request.duration_days),
            receiver_node_reserve_min: RECEIVER_NODE_RESERVE_MIN_CKB,
            receiver_node_reserve_recommended: RECEIVER_NODE_RESERVE_RECOMMENDED_CKB,
            routing_fee_bps: 30,
            available: available_liquidity >= request.amount,
            available_liquidity,
        })
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

        let signed_tx = request.signed_tx.clone();
        let request_tx_hash = normalize_transaction_hash(&request.request_tx_hash, &signed_tx);
        let request_cell_out_point = normalize_optional(&request.request_cell_out_point);
        let intent = if let Some(intent_id) = request.intent_id {
            validate_transaction_proof(&request.request_tx_hash, &signed_tx)?;
            if request_tx_hash.is_none() {
                return Err(anyhow!("request settlement requires tx_hash"));
            }
            Some(self.request_intent_for(user, intent_id).await?)
        } else {
            if request_tx_hash.is_some() || signed_tx.is_some() {
                return Err(anyhow!(
                    "request transaction settlement requires a request intent_id"
                ));
            }
            None
        };
        if let Some(intent) = intent.as_ref() {
            validate_pending_intent(&intent.status, intent.expires_at)?;
            require_intent_matches(intent, &request, &quote)?;
        }

        let now = Utc::now();
        let id = intent
            .as_ref()
            .map(|intent| intent.id)
            .unwrap_or_else(Uuid::new_v4);
        let liquidity_request = LiquidityRequest {
            id,
            merchant_id: user.id,
            merchant_name: user.display_name.clone(),
            ckb_address: user.ckb_address.clone(),
            asset: quote.asset,
            amount: request.amount,
            duration_days: request.duration_days,
            lease_fee: quote.lease_fee,
            routing_fee_bps: quote.routing_fee_bps,
            fiber_peer_pubkey: normalize_optional(&request.fiber_peer_pubkey),
            fiber_peer_address: normalize_optional(&request.fiber_peer_address),
            public_channel: request.public_channel.unwrap_or(false),
            funding_udt_type_script: request.funding_udt_type_script.clone(),
            request_cell_id: intent
                .as_ref()
                .map(|intent| intent.request_cell_id.clone())
                .unwrap_or_else(|| request_cell_id(id)),
            request_tx_hash: request_tx_hash.clone(),
            request_cell_out_point,
            status: LiquidityStatus::Requested,
            fiber_temporary_channel_id: None,
            channel_id: None,
            fiber_note: None,
            fiber_error: None,
            created_at: now,
            updated_at: now,
        };

        if intent.is_some() {
            self.verify_capacity_request_tx(&liquidity_request, &signed_tx)
                .await?;
        }

        let mut state = self.inner.write().await;
        let vault = state.vault_config(&self.vault);
        if state
            .vault_summary(&vault, liquidity_request.asset.clone())
            .available_liquidity
            < liquidity_request.amount
        {
            return Err(anyhow!("liquidity was just reserved by another request"));
        }
        reserve_positions_with_fee(
            &mut state.lp_positions,
            &liquidity_request.asset,
            liquidity_request.amount,
            liquidity_request.lease_fee,
            now,
        )?;
        if let Some(intent) = intent.as_ref()
            && let Some(stored) = state
                .request_intents
                .iter_mut()
                .find(|stored| stored.id == intent.id)
        {
            stored.status = IntentStatus::Settled;
            stored.tx_hash = request_tx_hash.clone();
        }
        if let Some(tx_hash) = request_tx_hash.as_deref() {
            state.vault_cell_out_point = Some(vault_output_out_point(tx_hash));
        }
        state
            .events
            .insert(0, reserve_event(user, &liquidity_request, now));
        state
            .capacity_reservations
            .push(reservation(user, &liquidity_request, now));
        state.liquidity_requests.push(liquidity_request.clone());
        self.persist_locked(&state).await?;
        drop(state);

        if let Some(executed) = self
            .try_execute_liquidity_request(liquidity_request.id)
            .await
        {
            return Ok(executed);
        }

        Ok(liquidity_request)
    }

    async fn request_intent_for(&self, user: &User, id: Uuid) -> Result<RequestIntent> {
        let state = self.inner.read().await;
        let intent = state
            .request_intents
            .iter()
            .find(|intent| intent.id == id)
            .ok_or_else(|| anyhow!("request intent not found"))?;
        if user.role != UserRole::Operator && intent.merchant_id != user.id {
            return Err(anyhow!("you can only settle your own request intent"));
        }
        Ok(intent.clone())
    }
}

fn require_intent_matches(
    intent: &RequestIntent,
    request: &CreateLiquidityRequest,
    quote: &LiquidityQuote,
) -> Result<()> {
    let same_request = intent.asset == quote.asset
        && intent.amount == request.amount
        && intent.duration_days == request.duration_days
        && intent.lease_fee == quote.lease_fee
        && intent.routing_fee_bps == quote.routing_fee_bps
        && intent.fiber_peer_pubkey == normalize_optional(&request.fiber_peer_pubkey)
        && intent.fiber_peer_address == normalize_optional(&request.fiber_peer_address)
        && intent.public_channel == request.public_channel.unwrap_or(false);
    if same_request {
        Ok(())
    } else {
        Err(anyhow!(
            "capacity request settlement does not match the intent"
        ))
    }
}

fn reservation(
    user: &User,
    liquidity_request: &LiquidityRequest,
    now: chrono::DateTime<Utc>,
) -> CapacityReservation {
    CapacityReservation {
        id: Uuid::new_v4(),
        request_id: liquidity_request.id,
        merchant_id: user.id,
        merchant_name: user.display_name.clone(),
        ckb_address: user.ckb_address.clone(),
        asset: liquidity_request.asset.clone(),
        amount: liquidity_request.amount,
        lease_fee: liquidity_request.lease_fee,
        request_cell_id: liquidity_request.request_cell_id.clone(),
        status: ReservationStatus::Reserved,
        created_at: now,
        updated_at: now,
    }
}

fn reserve_event(
    user: &User,
    request: &LiquidityRequest,
    now: chrono::DateTime<Utc>,
) -> ActivityEvent {
    ActivityEvent {
        id: Uuid::new_v4(),
        actor_id: user.id,
        label: format!("{} reserved receive capacity", user.display_name),
        amount: Some(request.amount),
        asset: Some(request.asset.clone()),
        created_at: now,
    }
}

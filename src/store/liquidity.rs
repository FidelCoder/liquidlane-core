use anyhow::{Result, anyhow};
use chrono::Utc;
use uuid::Uuid;

use super::{
    AppStore,
    accounting::{
        deploy_reserved_positions, release_reserved_positions, request_cell_id, reserve_positions,
    },
    validation::{
        lease_fee, normalize_asset, normalize_optional, require_role, validate_liquidity_request,
    },
};
use crate::domain::{
    ActivityEvent, CapacityReservation, CreateLiquidityRequest, LiquidityQuote, LiquidityRequest,
    LiquidityStatus, ReservationStatus, User, UserRole,
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
        state
            .events
            .insert(0, reserve_event(user, &liquidity_request, now));
        state
            .capacity_reservations
            .push(reservation(user, &liquidity_request, now));
        state.liquidity_requests.push(liquidity_request.clone());
        self.persist_locked(&state).await?;

        Ok(liquidity_request)
    }

    pub async fn deploy_liquidity(&self, user: &User, id: Uuid) -> Result<LiquidityRequest> {
        require_role(user, &[UserRole::Merchant, UserRole::Operator])?;
        let request = self.authorized_liquidity_request(user, id).await?;
        let outcome = self.fiber.open_channel(&request).await;

        let mut state = self.inner.write().await;
        let request = state
            .liquidity_requests
            .iter_mut()
            .find(|request| request.id == id)
            .ok_or_else(|| anyhow!("liquidity request not found"))?;

        let now = Utc::now();
        let event_label = match outcome {
            Ok(outcome) => {
                request.status = LiquidityStatus::PendingFiberChannel;
                request.fiber_temporary_channel_id = outcome.temporary_channel_id;
                request.channel_id = outcome.channel_id;
                request.fiber_note = outcome.note;
                request.fiber_error = None;
                request.updated_at = now;
                if outcome.rpc_submitted {
                    format!("Submitted Fiber open_channel for {}", request.merchant_name)
                } else {
                    format!("Queued Fiber channel open for {}", request.merchant_name)
                }
            }
            Err(error) => {
                request.status = LiquidityStatus::Failed;
                request.fiber_error = Some(error.to_string());
                request.fiber_note = None;
                request.updated_at = now;
                format!("Fiber channel open failed for {}", request.merchant_name)
            }
        };

        let updated = request.clone();
        update_reservation_and_positions(&mut state, &updated, user, now)?;
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

    async fn authorized_liquidity_request(
        &self,
        user: &User,
        id: Uuid,
    ) -> Result<LiquidityRequest> {
        let state = self.inner.read().await;
        let request = state
            .liquidity_requests
            .iter()
            .find(|request| request.id == id)
            .ok_or_else(|| anyhow!("liquidity request not found"))?;

        if user.role != UserRole::Operator && request.merchant_id != user.id {
            return Err(anyhow!("you can only open your own liquidity requests"));
        }
        Ok(request.clone())
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
        request_cell_id: request_cell_id(liquidity_request.id),
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

fn update_reservation_and_positions(
    state: &mut super::StoreState,
    updated: &LiquidityRequest,
    user: &User,
    now: chrono::DateTime<Utc>,
) -> Result<()> {
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
    Ok(())
}

use anyhow::{Result, anyhow};
use chrono::Utc;
use uuid::Uuid;

use super::{
    AppStore,
    accounting::{deploy_reserved_positions, release_reserved_positions},
    validation::require_role,
};
use crate::domain::{
    ActivityEvent, LiquidityRequest, LiquidityStatus, ReservationStatus, User, UserRole,
};

impl AppStore {
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

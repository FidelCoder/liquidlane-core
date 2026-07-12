use anyhow::{Result, anyhow};
use chrono::Utc;
use uuid::Uuid;

use super::AppStore;
use crate::domain::{LiquidityRequest, User, UserRole};

impl AppStore {
    pub(super) async fn stored_liquidity_request(&self, id: Uuid) -> Result<LiquidityRequest> {
        let state = self.inner.read().await;
        state
            .liquidity_requests
            .iter()
            .find(|request| request.id == id)
            .cloned()
            .ok_or_else(|| anyhow!("liquidity request not found"))
    }

    pub(super) async fn mark_executor_note(
        &self,
        id: Uuid,
        note: &str,
    ) -> Result<LiquidityRequest> {
        let mut state = self.inner.write().await;
        let request = state
            .liquidity_requests
            .iter_mut()
            .find(|request| request.id == id)
            .ok_or_else(|| anyhow!("liquidity request not found"))?;
        request.fiber_note = Some(note.to_string());
        request.fiber_error = None;
        request.updated_at = Utc::now();
        let updated = request.clone();
        self.persist_locked(&state).await?;
        Ok(updated)
    }

    pub(super) async fn authorized_liquidity_request(
        &self,
        user: &User,
        id: Uuid,
    ) -> Result<LiquidityRequest> {
        let request = self.stored_liquidity_request(id).await?;
        if user.role != UserRole::Operator && request.merchant_id != user.id {
            return Err(anyhow!("you can only open your own liquidity requests"));
        }
        Ok(request)
    }
}

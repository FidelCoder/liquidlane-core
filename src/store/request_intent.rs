use anyhow::{Result, anyhow};
use chrono::{Duration, Utc};
use uuid::Uuid;

use super::{
    AppStore,
    accounting::request_cell_id,
    validation::{normalize_optional, require_role},
};
use crate::domain::{CreateLiquidityRequest, IntentStatus, RequestIntent, User, UserRole};

impl AppStore {
    pub async fn create_request_intent(
        &self,
        user: &User,
        request: CreateLiquidityRequest,
    ) -> Result<RequestIntent> {
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
        let id = Uuid::new_v4();
        let intent = RequestIntent {
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
            funding_udt_type_script: request.funding_udt_type_script,
            request_cell_id: request_cell_id(id),
            memo: format!(
                "LL_REQUEST:{id}:{}:{}:{}",
                request.asset.trim().to_uppercase(),
                request.amount,
                request.duration_days
            ),
            status: IntentStatus::PendingSignature,
            tx_hash: None,
            created_at: now,
            expires_at: now + Duration::minutes(15),
        };

        let mut state = self.inner.write().await;
        state.request_intents.push(intent.clone());
        self.persist_locked(&state).await?;
        Ok(intent)
    }
}

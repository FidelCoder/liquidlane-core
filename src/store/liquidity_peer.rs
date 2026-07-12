use anyhow::{Result, anyhow};
use chrono::Utc;
use uuid::Uuid;

use super::AppStore;
use crate::domain::{ActivityEvent, AttachFiberPeerRequest, LiquidityRequest, User, UserRole};

impl AppStore {
    pub async fn attach_fiber_peer(
        &self,
        user: &User,
        id: Uuid,
        request: AttachFiberPeerRequest,
    ) -> Result<LiquidityRequest> {
        let peer_pubkey = normalize_pubkey(&request.fiber_peer_pubkey)?;
        let peer_address = normalize_peer_address(&request.fiber_peer_address)?;
        let mut state = self.inner.write().await;
        let stored = state
            .liquidity_requests
            .iter_mut()
            .find(|stored| stored.id == id)
            .ok_or_else(|| anyhow!("liquidity request not found"))?;
        if user.role != UserRole::Operator && stored.merchant_id != user.id {
            return Err(anyhow!("you can only update your own liquidity requests"));
        }
        stored.fiber_peer_pubkey = Some(peer_pubkey);
        stored.fiber_peer_address = peer_address;
        stored.public_channel = request.public_channel.unwrap_or(stored.public_channel);
        stored.fiber_note = Some(
            "Fiber peer details attached. LiquidLane executor can process this request."
                .to_string(),
        );
        stored.fiber_error = None;
        stored.updated_at = Utc::now();
        let updated = stored.clone();
        state.events.insert(
            0,
            ActivityEvent {
                id: Uuid::new_v4(),
                actor_id: user.id,
                label: format!("{} attached Fiber peer details", user.display_name),
                amount: Some(updated.amount),
                asset: Some(updated.asset.clone()),
                created_at: Utc::now(),
            },
        );
        self.persist_locked(&state).await?;
        drop(state);

        if let Some(executed) = self.try_execute_liquidity_request(updated.id).await {
            return Ok(executed);
        }

        Ok(updated)
    }
}

fn normalize_pubkey(value: &str) -> Result<String> {
    let value = value.trim();
    if is_fiber_pubkey(value) {
        Ok(value.to_string())
    } else {
        Err(anyhow!(
            "fiber_peer_pubkey must be a compressed 33-byte hex pubkey"
        ))
    }
}

fn normalize_peer_address(value: &Option<String>) -> Result<Option<String>> {
    let Some(value) = value
        .as_ref()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };
    if is_fiber_multiaddr(value) {
        Ok(Some(value.to_string()))
    } else {
        Err(anyhow!(
            "fiber_peer_address must be a Fiber multiaddr ending in /p2p/<peer_id>"
        ))
    }
}

fn is_fiber_pubkey(value: &str) -> bool {
    value.len() == 66
        && (value.starts_with("02") || value.starts_with("03"))
        && value.chars().all(|ch| ch.is_ascii_hexdigit())
}

fn is_fiber_multiaddr(value: &str) -> bool {
    let value = value.trim();
    value.starts_with('/') && value.contains("/p2p/") && !value.ends_with("/p2p/")
}

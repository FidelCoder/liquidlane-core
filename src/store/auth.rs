use anyhow::{Result, anyhow};
use chrono::{Duration, Utc};
use uuid::Uuid;

use super::{
    AppStore,
    validation::{
        normalize_ckb_address, normalize_wallet_type, short_ckb_address, validate_script,
        validate_wallet_proof,
    },
};
use crate::domain::{
    ActivityEvent, AuthChallenge, AuthResponse, ChallengeRequest, ChallengeResponse,
    ConnectWalletRequest, User, UserProfile, VerifyWalletRequest,
};

impl AppStore {
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
        let user = upsert_user(
            &mut state.users,
            &ckb_address,
            &wallet_type,
            display_name.trim(),
            request.role,
            request.lock_script,
            now,
        );

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
            validate_challenge(challenge, &ckb_address, &wallet_type)?;
            challenge.consumed = true;
            challenge.role.clone()
        };

        let now = Utc::now();
        let display_name = request
            .display_name
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| short_ckb_address(&ckb_address));
        let user = upsert_user(
            &mut state.users,
            &ckb_address,
            &wallet_type,
            display_name.trim(),
            role,
            request.lock_script,
            now,
        );

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
}

fn upsert_user(
    users: &mut Vec<User>,
    ckb_address: &str,
    wallet_type: &str,
    display_name: &str,
    role: crate::domain::UserRole,
    lock_script: Option<crate::domain::CkbScript>,
    now: chrono::DateTime<Utc>,
) -> User {
    if let Some(index) = users
        .iter()
        .position(|user| user.ckb_address == ckb_address && user.wallet_type == wallet_type)
    {
        users[index].display_name = display_name.to_string();
        users[index].role = role;
        users[index].token = Uuid::new_v4().to_string();
        users[index].lock_script = lock_script;
        return users[index].clone();
    }

    let user = User {
        id: Uuid::new_v4(),
        display_name: display_name.to_string(),
        ckb_address: ckb_address.to_string(),
        wallet_type: wallet_type.to_string(),
        lock_script,
        role,
        token: Uuid::new_v4().to_string(),
        created_at: now,
    };
    users.push(user.clone());
    user
}

fn validate_challenge(
    challenge: &AuthChallenge,
    ckb_address: &str,
    wallet_type: &str,
) -> Result<()> {
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
    Ok(())
}

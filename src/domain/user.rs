use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::CkbScript;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct User {
    pub id: Uuid,
    pub display_name: String,
    #[serde(alias = "wallet_address")]
    pub ckb_address: String,
    #[serde(default = "default_wallet_type")]
    pub wallet_type: String,
    #[serde(default)]
    pub lock_script: Option<CkbScript>,
    pub role: UserRole,
    pub token: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UserRole {
    Lp,
    Merchant,
    Operator,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ChallengeRequest {
    #[serde(alias = "wallet_address")]
    pub ckb_address: String,
    #[serde(default = "default_wallet_type")]
    pub wallet_type: String,
    pub role: UserRole,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ConnectWalletRequest {
    #[serde(alias = "wallet_address")]
    pub ckb_address: String,
    #[serde(default = "default_wallet_type")]
    pub wallet_type: String,
    pub role: UserRole,
    #[serde(default)]
    pub lock_script: Option<CkbScript>,
    pub display_name: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ChallengeResponse {
    pub challenge_id: Uuid,
    pub message: String,
    pub expires_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct VerifyWalletRequest {
    pub challenge_id: Uuid,
    #[serde(alias = "wallet_address")]
    pub ckb_address: String,
    #[serde(default = "default_wallet_type")]
    pub wallet_type: String,
    pub signature: String,
    #[serde(default)]
    pub lock_script: Option<CkbScript>,
    pub display_name: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct AuthResponse {
    pub token: String,
    pub user: UserProfile,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuthChallenge {
    pub id: Uuid,
    #[serde(alias = "wallet_address")]
    pub ckb_address: String,
    #[serde(default = "default_wallet_type")]
    pub wallet_type: String,
    pub role: UserRole,
    pub message: String,
    pub expires_at: DateTime<Utc>,
    pub consumed: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct UserProfile {
    pub id: Uuid,
    pub display_name: String,
    pub ckb_address: String,
    pub wallet_type: String,
    pub role: UserRole,
}

impl From<&User> for UserProfile {
    fn from(user: &User) -> Self {
        Self {
            id: user.id,
            display_name: user.display_name.clone(),
            ckb_address: user.ckb_address.clone(),
            wallet_type: user.wallet_type.clone(),
            role: user.role.clone(),
        }
    }
}

fn default_wallet_type() -> String {
    "ckb".to_string()
}

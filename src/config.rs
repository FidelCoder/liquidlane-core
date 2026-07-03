use std::{env, net::SocketAddr, path::PathBuf};

use crate::domain::VaultConfig;

#[derive(Clone, Debug)]
pub struct AppConfig {
    pub bind_addr: SocketAddr,
    pub environment: String,
    pub data_path: PathBuf,
    pub fiber_rpc_url: Option<String>,
    pub fiber_rpc_auth_token: Option<String>,
    pub vault: VaultConfig,
}

impl AppConfig {
    pub fn from_env() -> anyhow::Result<Self> {
        let bind_addr = env::var("LIQUIDLANE_BIND_ADDR")
            .unwrap_or_else(|_| "0.0.0.0:8080".to_string())
            .parse()?;
        let environment = env::var("LIQUIDLANE_ENV").unwrap_or_else(|_| "development".to_string());
        let data_path = env::var("LIQUIDLANE_DATA_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("./liquidlane-data.json"));
        let fiber_rpc_url = env::var("FIBER_RPC_URL")
            .ok()
            .filter(|value| !value.trim().is_empty());
        let fiber_rpc_auth_token = env::var("FIBER_RPC_AUTH_TOKEN")
            .ok()
            .filter(|value| !value.trim().is_empty());
        let vault_address = env::var("LIQUIDLANE_VAULT_CKB_ADDRESS")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        let vault = VaultConfig {
            asset: env::var("LIQUIDLANE_VAULT_ASSET")
                .unwrap_or_else(|_| "CKB".to_string())
                .trim()
                .to_uppercase(),
            configured: vault_address.is_some(),
            address: vault_address,
            network: env::var("LIQUIDLANE_CKB_NETWORK")
                .unwrap_or_else(|_| "testnet".to_string())
                .trim()
                .to_lowercase(),
        };

        Ok(Self {
            bind_addr,
            environment,
            data_path,
            fiber_rpc_url,
            fiber_rpc_auth_token,
            vault,
        })
    }
}

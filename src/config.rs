use std::{env, net::SocketAddr, path::PathBuf};

use crate::domain::{VaultConfig, VaultScripts, is_plausible_ckb_address};

#[derive(Clone, Debug)]
pub struct AppConfig {
    pub bind_addr: SocketAddr,
    pub environment: String,
    pub data_path: PathBuf,
    pub fiber_rpc_url: Option<String>,
    pub fiber_rpc_auth_token: Option<String>,
    pub ckb_rpc_url: Option<String>,
    pub ckb_accept_pending_txs: bool,
    pub require_ckb_rpc: bool,
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
        let ckb_rpc_url = optional_env("LIQUIDLANE_CKB_RPC_URL");
        let ckb_accept_pending_txs = bool_env("LIQUIDLANE_CKB_ACCEPT_PENDING_TXS", false)?;
        let vault_address = optional_env("LIQUIDLANE_VAULT_CKB_ADDRESS")
            .map(validate_vault_address)
            .transpose()?;
        let require_ckb_rpc = bool_env(
            "LIQUIDLANE_REQUIRE_CKB_RPC",
            environment != "development" && environment != "test",
        )?;
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
            scripts: VaultScripts {
                vault_lock_code_hash: optional_env("LIQUIDLANE_VAULT_LOCK_CODE_HASH"),
                vault_type_code_hash: optional_env("LIQUIDLANE_VAULT_TYPE_CODE_HASH"),
                lp_receipt_type_code_hash: optional_env("LIQUIDLANE_LP_RECEIPT_TYPE_CODE_HASH"),
                request_type_code_hash: optional_env("LIQUIDLANE_REQUEST_TYPE_CODE_HASH"),
                fee_claim_type_code_hash: optional_env("LIQUIDLANE_FEE_CLAIM_TYPE_CODE_HASH"),
            },
        };

        Ok(Self {
            bind_addr,
            environment,
            data_path,
            fiber_rpc_url,
            fiber_rpc_auth_token,
            ckb_rpc_url,
            ckb_accept_pending_txs,
            require_ckb_rpc,
            vault,
        })
    }
}

fn optional_env(key: &str) -> Option<String> {
    env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn validate_vault_address(address: String) -> anyhow::Result<String> {
    if is_plausible_ckb_address(&address) {
        return Ok(address);
    }

    anyhow::bail!(
        "LIQUIDLANE_VAULT_CKB_ADDRESS must be a real CKB address from a wallet or vault script, not a placeholder"
    )
}

fn bool_env(key: &str, default: bool) -> anyhow::Result<bool> {
    let Some(value) = optional_env(key) else {
        return Ok(default);
    };
    match value.trim().to_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        _ => anyhow::bail!("{key} must be a boolean"),
    }
}

use std::{env, net::SocketAddr, path::PathBuf};

use crate::domain::{
    FUNDING_MODE_VAULT_EXTERNAL, VaultConfig, VaultScripts, is_plausible_ckb_address,
    normalize_executor_funding_mode,
};

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
    pub executor_enabled: bool,
    pub executor_poll_interval_ms: u64,
    pub executor_max_retries: u8,
    pub executor_funding_mode: String,
    pub vault_funding_builder_enabled: bool,
    pub vault_funding_signer_enabled: bool,
    pub ckb_script_build_dir: PathBuf,
    pub cors_allowed_origin: Option<String>,
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
        let fiber_rpc_url =
            optional_env("FIBER_RPC_URL").or_else(|| production_fiber_rpc_url(&environment));
        let fiber_rpc_auth_token = env::var("FIBER_RPC_AUTH_TOKEN")
            .ok()
            .filter(|value| !value.trim().is_empty());
        let ckb_rpc_url = optional_env("LIQUIDLANE_CKB_RPC_URL");
        let ckb_network = env::var("LIQUIDLANE_CKB_NETWORK")
            .unwrap_or_else(|_| "testnet".to_string())
            .trim()
            .to_lowercase();
        if !is_testnet_network(&ckb_network) {
            anyhow::bail!(
                "LiquidLane beta is locked to CKB testnet. Set LIQUIDLANE_CKB_NETWORK=testnet"
            );
        }
        let ckb_accept_pending_txs = ckb_accept_pending_txs(&ckb_network)?;
        let ckb_script_build_dir = env::var("LIQUIDLANE_CKB_SCRIPT_BUILD_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("./ckb-scripts/build"));
        let vault_address = optional_env("LIQUIDLANE_VAULT_CKB_ADDRESS")
            .map(validate_vault_address)
            .transpose()?;
        let vault_cell_out_point = optional_env("LIQUIDLANE_VAULT_CELL_OUT_POINT")
            .map(validate_out_point)
            .transpose()?;
        let require_ckb_rpc = bool_env(
            "LIQUIDLANE_REQUIRE_CKB_RPC",
            environment != "development" && environment != "test",
        )?;
        let executor_enabled = bool_env("LIQUIDLANE_EXECUTOR_ENABLED", environment != "test")?;
        let executor_poll_interval_ms = u64_env("LIQUIDLANE_EXECUTOR_POLL_INTERVAL_MS", 5_000)?;
        let executor_max_retries = u8_env("LIQUIDLANE_EXECUTOR_MAX_RETRIES", 3)?;
        let executor_funding_mode = normalize_executor_funding_mode(
            &optional_env("LIQUIDLANE_FIBER_FUNDING_MODE")
                .or_else(|| optional_env("LIQUIDLANE_EXECUTOR_FUNDING_MODE"))
                .unwrap_or_else(|| FUNDING_MODE_VAULT_EXTERNAL.to_string()),
        );
        let vault_funding_builder_enabled =
            bool_env("LIQUIDLANE_VAULT_FUNDING_BUILDER_ENABLED", false)?;
        let vault_funding_signer_enabled =
            bool_env("LIQUIDLANE_VAULT_FUNDING_SIGNER_ENABLED", false)?;
        let cors_allowed_origin = optional_env("LIQUIDLANE_CORS_ALLOWED_ORIGIN");
        let vault = VaultConfig {
            asset: env::var("LIQUIDLANE_VAULT_ASSET")
                .unwrap_or_else(|_| "CKB".to_string())
                .trim()
                .to_uppercase(),
            configured: vault_address.is_some() && vault_cell_out_point.is_some(),
            address: vault_address,
            cell_out_point: vault_cell_out_point,
            network: ckb_network,
            script_version: optional_env("LIQUIDLANE_VAULT_SCRIPT_VERSION")
                .unwrap_or_else(|| "v1".to_string())
                .to_ascii_lowercase(),
            scripts: VaultScripts {
                vault_lock_code_hash: optional_env("LIQUIDLANE_VAULT_LOCK_CODE_HASH"),
                vault_lock_out_point: optional_env("LIQUIDLANE_VAULT_LOCK_OUT_POINT"),
                vault_type_code_hash: optional_env("LIQUIDLANE_VAULT_TYPE_CODE_HASH"),
                vault_type_out_point: optional_env("LIQUIDLANE_VAULT_TYPE_OUT_POINT"),
                lp_receipt_type_code_hash: optional_env("LIQUIDLANE_LP_RECEIPT_TYPE_CODE_HASH"),
                lp_receipt_type_out_point: optional_env("LIQUIDLANE_LP_RECEIPT_TYPE_OUT_POINT"),
                request_type_code_hash: optional_env("LIQUIDLANE_REQUEST_TYPE_CODE_HASH"),
                request_type_out_point: optional_env("LIQUIDLANE_REQUEST_TYPE_OUT_POINT"),
                funding_intent_type_code_hash: optional_env(
                    "LIQUIDLANE_FUNDING_INTENT_TYPE_CODE_HASH",
                ),
                funding_intent_type_out_point: optional_env(
                    "LIQUIDLANE_FUNDING_INTENT_TYPE_OUT_POINT",
                ),
                fee_claim_type_code_hash: optional_env("LIQUIDLANE_FEE_CLAIM_TYPE_CODE_HASH"),
                fee_claim_type_out_point: optional_env("LIQUIDLANE_FEE_CLAIM_TYPE_OUT_POINT"),
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
            executor_enabled,
            executor_poll_interval_ms,
            executor_max_retries,
            executor_funding_mode,
            vault_funding_builder_enabled,
            vault_funding_signer_enabled,
            ckb_script_build_dir,
            cors_allowed_origin,
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

fn production_fiber_rpc_url(environment: &str) -> Option<String> {
    (environment.trim().eq_ignore_ascii_case("production"))
        .then(|| "https://liquidlane-fiber.onrender.com".to_string())
}

fn ckb_accept_pending_txs(network: &str) -> anyhow::Result<bool> {
    if is_testnet_network(network) {
        return Ok(true);
    }
    bool_env("LIQUIDLANE_CKB_ACCEPT_PENDING_TXS", false)
}

fn is_testnet_network(network: &str) -> bool {
    matches!(
        network.trim().to_ascii_lowercase().as_str(),
        "testnet" | "ckb-testnet" | "pudge" | "pudge-testnet"
    )
}

fn validate_vault_address(address: String) -> anyhow::Result<String> {
    if address.trim().starts_with("ckt1") && is_plausible_ckb_address(&address) {
        return Ok(address);
    }

    anyhow::bail!(
        "LIQUIDLANE_VAULT_CKB_ADDRESS must be a real CKB testnet address from a wallet or vault script, not a placeholder"
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

fn u64_env(key: &str, default: u64) -> anyhow::Result<u64> {
    let Some(value) = optional_env(key) else {
        return Ok(default);
    };
    value
        .parse()
        .map_err(|_| anyhow::anyhow!("{key} must be an unsigned integer"))
}

fn u8_env(key: &str, default: u8) -> anyhow::Result<u8> {
    let Some(value) = optional_env(key) else {
        return Ok(default);
    };
    value
        .parse()
        .map_err(|_| anyhow::anyhow!("{key} must be an unsigned integer from 0 to 255"))
}

fn validate_out_point(value: String) -> anyhow::Result<String> {
    let value = value.trim().to_string();
    let Some((tx_hash, index)) = value.split_once('#') else {
        anyhow::bail!("CKB out-point must use tx_hash#index format");
    };
    if tx_hash.len() != 66 || !tx_hash.starts_with("0x") {
        anyhow::bail!("CKB out-point tx_hash must be a 0x-prefixed 32-byte hash");
    }
    if !index.starts_with("0x") || index.len() < 3 {
        anyhow::bail!("CKB out-point index must be 0x-prefixed");
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::is_testnet_network;

    #[test]
    fn treats_pudge_as_testnet() {
        assert!(is_testnet_network("testnet"));
        assert!(is_testnet_network("ckb-testnet"));
        assert!(is_testnet_network("pudge-testnet"));
        assert!(!is_testnet_network("mainnet"));
    }
}

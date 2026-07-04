use std::{collections::HashMap, env, fs, path::PathBuf};

use anyhow::{Context, Result, anyhow};

#[derive(Clone, Debug)]
pub struct DeployConfig {
    pub network: String,
    pub rpc_url: String,
    pub deployer_address: String,
    pub private_key: String,
    pub build_dir: PathBuf,
    pub deployments_dir: PathBuf,
    pub spend_previous_script_cells: bool,
    pub scripts: DeployedScripts,
}

#[allow(dead_code)]
#[derive(Clone, Debug, Default)]
pub struct DeployedScripts {
    pub vault_lock_code_hash: Option<String>,
    pub vault_lock_out_point: Option<String>,
    pub vault_type_code_hash: Option<String>,
    pub vault_type_out_point: Option<String>,
    pub lp_receipt_type_code_hash: Option<String>,
    pub lp_receipt_type_out_point: Option<String>,
    pub request_type_code_hash: Option<String>,
    pub request_type_out_point: Option<String>,
    pub fee_claim_type_code_hash: Option<String>,
    pub fee_claim_type_out_point: Option<String>,
}

impl DeployConfig {
    pub fn load() -> Result<Self> {
        let local_env = read_local_env(PathBuf::from(".env"))?;
        let network = value(&local_env, "CKB_NETWORK")
            .or_else(|| value(&local_env, "LIQUIDLANE_CKB_NETWORK"))
            .unwrap_or_else(|| "testnet".to_string());
        if network != "testnet" {
            return Err(anyhow!("terminal deployer is locked to testnet"));
        }

        let rpc_url = value(&local_env, "CKB_RPC_URL")
            .or_else(|| value(&local_env, "LIQUIDLANE_CKB_RPC_URL"))
            .unwrap_or_else(|| "https://testnet.ckb.dev".to_string());
        let deployer_address = value(&local_env, "CKB_DEPLOYER_ADDRESS")
            .context("CKB_DEPLOYER_ADDRESS is missing from .env")?;
        let private_key = value(&local_env, "CKB_DEPLOYER_PRIVATE_KEY")
            .context("CKB_DEPLOYER_PRIVATE_KEY is missing from .env")?;

        Ok(Self {
            network,
            rpc_url: normalize_rpc_url(&rpc_url),
            deployer_address,
            private_key,
            build_dir: path_value(&local_env, "CKB_SCRIPT_BUILD_DIR", "ckb-scripts/build"),
            deployments_dir: path_value(
                &local_env,
                "CKB_DEPLOYMENTS_DIR",
                "ckb-scripts/deployments",
            ),
            spend_previous_script_cells: bool_value(
                &local_env,
                "LIQUIDLANE_SPEND_PREVIOUS_SCRIPT_CELLS",
                false,
            )?,
            scripts: DeployedScripts {
                vault_lock_code_hash: value(&local_env, "LIQUIDLANE_VAULT_LOCK_CODE_HASH"),
                vault_lock_out_point: value(&local_env, "LIQUIDLANE_VAULT_LOCK_OUT_POINT"),
                vault_type_code_hash: value(&local_env, "LIQUIDLANE_VAULT_TYPE_CODE_HASH"),
                vault_type_out_point: value(&local_env, "LIQUIDLANE_VAULT_TYPE_OUT_POINT"),
                lp_receipt_type_code_hash: value(
                    &local_env,
                    "LIQUIDLANE_LP_RECEIPT_TYPE_CODE_HASH",
                ),
                lp_receipt_type_out_point: value(
                    &local_env,
                    "LIQUIDLANE_LP_RECEIPT_TYPE_OUT_POINT",
                ),
                request_type_code_hash: value(&local_env, "LIQUIDLANE_REQUEST_TYPE_CODE_HASH"),
                request_type_out_point: value(&local_env, "LIQUIDLANE_REQUEST_TYPE_OUT_POINT"),
                fee_claim_type_code_hash: value(&local_env, "LIQUIDLANE_FEE_CLAIM_TYPE_CODE_HASH"),
                fee_claim_type_out_point: value(&local_env, "LIQUIDLANE_FEE_CLAIM_TYPE_OUT_POINT"),
            },
        })
    }
}

fn read_local_env(path: PathBuf) -> Result<HashMap<String, String>> {
    let mut values = HashMap::new();
    if let Ok(contents) = fs::read_to_string(&path) {
        for line in contents.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some((key, raw_value)) = line.split_once('=') {
                values.insert(key.trim().to_string(), unquote(raw_value.trim()));
            }
        }
    }
    Ok(values)
}

fn value(local_env: &HashMap<String, String>, key: &str) -> Option<String> {
    env::var(key)
        .ok()
        .or_else(|| local_env.get(key).cloned())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty() && value != "0x...")
}

fn bool_value(local_env: &HashMap<String, String>, key: &str, default: bool) -> Result<bool> {
    match value(local_env, key) {
        Some(value) => match value.to_ascii_lowercase().as_str() {
            "true" | "1" | "yes" => Ok(true),
            "false" | "0" | "no" => Ok(false),
            _ => Err(anyhow!("{key} must be true or false")),
        },
        None => Ok(default),
    }
}

fn path_value(local_env: &HashMap<String, String>, key: &str, default: &str) -> PathBuf {
    value(local_env, key)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(default))
}

fn normalize_rpc_url(value: &str) -> String {
    value
        .trim_end_matches('/')
        .trim_end_matches("/rpc")
        .trim_end_matches("/indexer")
        .to_string()
}

fn unquote(value: &str) -> String {
    value
        .strip_prefix('"')
        .and_then(|inner| inner.strip_suffix('"'))
        .unwrap_or(value)
        .to_string()
}

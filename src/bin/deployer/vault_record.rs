use std::{fs, path::PathBuf};

use anyhow::Result;
use chrono::Utc;
use serde::Serialize;

use super::{chain::CodeOutPoint, config::DeployConfig};

#[derive(Debug, Serialize)]
struct VaultRecord {
    network: String,
    deployed_at: String,
    deployer_address: String,
    vault_address: String,
    vault_cell_out_point: CodeOutPoint,
    vault_lock_script_hash: String,
    vault_type_script_hash: String,
    deployment_tx_hash: String,
    explorer_url: String,
}

pub(super) fn write_record(
    config: &DeployConfig,
    tx_hash: &str,
    vault_address: &str,
    vault_out_point: &CodeOutPoint,
    vault_lock_script_hash: &str,
    vault_type_script_hash: &str,
) -> Result<PathBuf> {
    fs::create_dir_all(&config.deployments_dir)?;
    let path = config.deployments_dir.join(format!(
        "vault-testnet-{}-{}.json",
        Utc::now().format("%Y-%m-%d"),
        tx_hash
            .trim_start_matches("0x")
            .chars()
            .take(12)
            .collect::<String>()
    ));
    let record = VaultRecord {
        network: "ckb-testnet".to_string(),
        deployed_at: Utc::now().to_rfc3339(),
        deployer_address: config.deployer_address.clone(),
        vault_address: vault_address.to_string(),
        vault_cell_out_point: vault_out_point.clone(),
        vault_lock_script_hash: vault_lock_script_hash.to_string(),
        vault_type_script_hash: vault_type_script_hash.to_string(),
        deployment_tx_hash: tx_hash.to_string(),
        explorer_url: format!("https://pudge.explorer.nervos.org/transaction/{tx_hash}"),
    };
    fs::write(
        &path,
        format!("{}\n", serde_json::to_string_pretty(&record)?),
    )?;
    Ok(path)
}

pub(super) fn write_env(vault_address: &str, vault_out_point: &CodeOutPoint) -> Result<()> {
    let path = PathBuf::from(".env");
    let mut env = fs::read_to_string(&path).unwrap_or_default();
    env = set_env(env, "LIQUIDLANE_VAULT_CKB_ADDRESS", vault_address);
    env = set_env(
        env,
        "LIQUIDLANE_VAULT_CELL_OUT_POINT",
        &format!("{}#{}", vault_out_point.tx_hash, vault_out_point.index),
    );
    env = set_env(env, "LIQUIDLANE_REQUIRE_CKB_RPC", "true");
    env = set_env(env, "LIQUIDLANE_VAULT_SCRIPT_VERSION", "v2");
    fs::write(path, env)?;
    Ok(())
}

fn set_env(text: String, key: &str, value: &str) -> String {
    let prefix = format!("{key}=");
    let mut lines = text.lines().map(str::to_string).collect::<Vec<_>>();
    if let Some(line) = lines.iter_mut().find(|line| line.starts_with(&prefix)) {
        *line = format!("{prefix}{value}");
    } else {
        lines.push(format!("{prefix}{value}"));
    }
    format!("{}\n", lines.join("\n"))
}

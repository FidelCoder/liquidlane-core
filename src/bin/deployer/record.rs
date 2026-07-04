use std::{fs, path::PathBuf};

use anyhow::{Context, Result};
use chrono::Utc;
use serde::Serialize;

use super::{chain::DeployReceipt, config::DeployConfig, scripts::ScriptArtifact};

#[derive(Debug, Serialize)]
struct DeploymentRecord {
    network: String,
    rpc_url: String,
    explorer_base_url: String,
    deployed_at: String,
    deployer_address: String,
    deployment_tx_hash: String,
    explorer_url: String,
    scripts: Vec<DeploymentScriptRecord>,
}

#[derive(Debug, Serialize)]
struct DeploymentScriptRecord {
    name: String,
    size_bytes: u64,
    deployment_tx_hash: String,
    code_cell_out_point: super::chain::CodeOutPoint,
    code_hash: String,
    hash_type: String,
    explorer_url: String,
}

pub fn write_record(
    config: &DeployConfig,
    scripts: &[ScriptArtifact],
    receipt: &DeployReceipt,
) -> Result<PathBuf> {
    fs::create_dir_all(&config.deployments_dir)
        .with_context(|| format!("could not create {}", config.deployments_dir.display()))?;
    let path = config
        .deployments_dir
        .join(record_file_name(&receipt.tx_hash));
    let record = build_record(config, scripts, receipt);
    let json = serde_json::to_string_pretty(&record)?;
    fs::write(
        &path,
        format!(
            "{json}
"
        ),
    )
    .with_context(|| format!("could not write {}", path.display()))?;
    Ok(path)
}

fn build_record(
    config: &DeployConfig,
    scripts: &[ScriptArtifact],
    receipt: &DeployReceipt,
) -> DeploymentRecord {
    let explorer_base_url = "https://pudge.explorer.nervos.org".to_string();
    let explorer_url = format!("{explorer_base_url}/transaction/{}", receipt.tx_hash);
    let scripts = scripts
        .iter()
        .zip(receipt.scripts.iter())
        .map(|(artifact, deployed)| DeploymentScriptRecord {
            name: artifact.name.clone(),
            size_bytes: artifact.size_bytes,
            deployment_tx_hash: receipt.tx_hash.clone(),
            code_cell_out_point: deployed.out_point.clone(),
            code_hash: artifact.ckb_data_hash.clone(),
            hash_type: artifact.hash_type.clone(),
            explorer_url: explorer_url.clone(),
        })
        .collect();

    DeploymentRecord {
        network: "ckb-testnet".to_string(),
        rpc_url: config.rpc_url.clone(),
        explorer_base_url,
        deployed_at: Utc::now().to_rfc3339(),
        deployer_address: config.deployer_address.clone(),
        deployment_tx_hash: receipt.tx_hash.clone(),
        explorer_url,
        scripts,
    }
}

fn record_file_name(tx_hash: &str) -> String {
    let short = tx_hash
        .trim_start_matches("0x")
        .chars()
        .take(12)
        .collect::<String>();
    format!("testnet-{}-{short}.json", Utc::now().format("%Y-%m-%d"))
}

pub fn write_env(scripts: &[ScriptArtifact], receipt: &DeployReceipt) -> Result<()> {
    let path = PathBuf::from(".env");
    let mut env = fs::read_to_string(&path).unwrap_or_default();
    env = set_env(
        env,
        "LIQUIDLANE_SCRIPT_DEPLOYMENT_TX_HASH",
        &receipt.tx_hash,
    );
    for (artifact, deployed) in scripts.iter().zip(receipt.scripts.iter()) {
        if let Some((hash_key, out_point_key)) = env_keys(&artifact.name) {
            env = set_env(env, hash_key, &artifact.ckb_data_hash);
            env = set_env(
                env,
                out_point_key,
                &format!(
                    "{}#{}",
                    deployed.out_point.tx_hash, deployed.out_point.index
                ),
            );
        }
    }
    fs::write(path, env)?;
    Ok(())
}

fn env_keys(name: &str) -> Option<(&'static str, &'static str)> {
    match name {
        "liquidlane-vault-lock" => Some((
            "LIQUIDLANE_VAULT_LOCK_CODE_HASH",
            "LIQUIDLANE_VAULT_LOCK_OUT_POINT",
        )),
        "liquidlane-vault-type" => Some((
            "LIQUIDLANE_VAULT_TYPE_CODE_HASH",
            "LIQUIDLANE_VAULT_TYPE_OUT_POINT",
        )),
        "liquidlane-lp-receipt-type" => Some((
            "LIQUIDLANE_LP_RECEIPT_TYPE_CODE_HASH",
            "LIQUIDLANE_LP_RECEIPT_TYPE_OUT_POINT",
        )),
        "liquidlane-capacity-request-type" => Some((
            "LIQUIDLANE_REQUEST_TYPE_CODE_HASH",
            "LIQUIDLANE_REQUEST_TYPE_OUT_POINT",
        )),
        "liquidlane-fee-claim-type" => Some((
            "LIQUIDLANE_FEE_CLAIM_TYPE_CODE_HASH",
            "LIQUIDLANE_FEE_CLAIM_TYPE_OUT_POINT",
        )),
        _ => None,
    }
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

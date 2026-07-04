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

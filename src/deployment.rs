use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use tokio::fs;

#[derive(Clone, Debug, Serialize)]
pub struct DeploymentPackage {
    pub network: String,
    pub scripts: Vec<DeploymentScript>,
}

#[derive(Clone, Debug, Serialize)]
pub struct DeploymentScript {
    pub name: String,
    pub size_bytes: u64,
    pub ckb_data_hash: String,
    pub hash_type: String,
    pub data_hex: String,
}

#[derive(Debug, Deserialize)]
struct Manifest {
    network: String,
    scripts: Vec<ManifestScript>,
}

#[derive(Debug, Deserialize)]
struct ManifestScript {
    name: String,
    path: String,
    size_bytes: u64,
    ckb_data_hash: String,
    hash_type: String,
}

pub async fn load_script_package(build_dir: &Path) -> Result<DeploymentPackage> {
    let manifest_path = build_dir.join("manifest.json");
    let manifest = fs::read_to_string(&manifest_path)
        .await
        .with_context(|| format!("could not read {}", manifest_path.display()))?;
    let manifest: Manifest = serde_json::from_str(&manifest)
        .with_context(|| format!("could not parse {}", manifest_path.display()))?;

    if manifest.scripts.is_empty() {
        return Err(anyhow!("CKB script manifest has no scripts"));
    }

    let mut scripts = Vec::with_capacity(manifest.scripts.len());
    for script in manifest.scripts {
        let artifact_path = artifact_path(build_dir, &script.path)?;
        let data = fs::read(&artifact_path)
            .await
            .with_context(|| format!("could not read {}", artifact_path.display()))?;
        if data.len() as u64 != script.size_bytes {
            return Err(anyhow!(
                "{} size mismatch: manifest={}, actual={}",
                script.name,
                script.size_bytes,
                data.len()
            ));
        }
        scripts.push(DeploymentScript {
            name: script.name,
            size_bytes: script.size_bytes,
            ckb_data_hash: script.ckb_data_hash,
            hash_type: script.hash_type,
            data_hex: hex_data(&data),
        });
    }

    Ok(DeploymentPackage {
        network: manifest.network,
        scripts,
    })
}

fn artifact_path(build_dir: &Path, manifest_path: &str) -> Result<PathBuf> {
    let file_name = Path::new(manifest_path)
        .file_name()
        .ok_or_else(|| anyhow!("script artifact path is missing a file name"))?;
    Ok(build_dir.join(file_name))
}

fn hex_data(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(2 + bytes.len() * 2);
    output.push_str("0x");
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

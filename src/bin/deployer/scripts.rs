use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, anyhow};
use serde::Deserialize;

#[derive(Clone, Debug)]
pub struct ScriptArtifact {
    pub name: String,
    pub size_bytes: u64,
    pub ckb_data_hash: String,
    pub hash_type: String,
    pub data: Vec<u8>,
}

#[derive(Debug, Deserialize)]
struct Manifest {
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

pub fn load_script_artifacts(build_dir: &Path) -> Result<Vec<ScriptArtifact>> {
    let manifest_path = build_dir.join("manifest.json");
    let manifest = fs::read_to_string(&manifest_path)
        .with_context(|| format!("could not read {}", manifest_path.display()))?;
    let manifest: Manifest = serde_json::from_str(&manifest)
        .with_context(|| format!("could not parse {}", manifest_path.display()))?;
    if manifest.scripts.is_empty() {
        return Err(anyhow!("CKB script manifest has no scripts"));
    }

    manifest
        .scripts
        .into_iter()
        .map(|script| load_artifact(build_dir, script))
        .collect()
}

fn load_artifact(build_dir: &Path, script: ManifestScript) -> Result<ScriptArtifact> {
    let path = artifact_path(build_dir, &script.path)?;
    let data = fs::read(&path).with_context(|| format!("could not read {}", path.display()))?;
    if data.len() as u64 != script.size_bytes {
        return Err(anyhow!(
            "{} size mismatch: manifest={}, actual={}",
            script.name,
            script.size_bytes,
            data.len()
        ));
    }
    Ok(ScriptArtifact {
        name: script.name,
        size_bytes: script.size_bytes,
        ckb_data_hash: script.ckb_data_hash,
        hash_type: script.hash_type,
        data,
    })
}

fn artifact_path(build_dir: &Path, manifest_path: &str) -> Result<PathBuf> {
    let file_name = Path::new(manifest_path)
        .file_name()
        .ok_or_else(|| anyhow!("script artifact path is missing a file name"))?;
    Ok(build_dir.join(file_name))
}

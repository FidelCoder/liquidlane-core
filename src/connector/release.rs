//! A pinned public Fiber release. Never execute an unverified downloaded binary.
use super::service;
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

const ARCHIVE: &str = "fnn_v0.9.0-x86_64-linux-portable.tar.gz";
const DIGEST: &str = "4085453de9a3f7ca0f0aeb7db9e34c0af4d34feb84566be554a5c70557cecbea";
#[derive(Serialize, Deserialize)]
struct Manifest {
    binary_sha256: String,
    config_sha256: String,
}
pub fn digest(path: &Path) -> Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0; 65536];
    loop {
        let size = file.read(&mut buffer)?;
        if size == 0 {
            break;
        }
        hasher.update(&buffer[..size]);
    }
    Ok(hex::encode(hasher.finalize()))
}
pub fn verify(release: &Path) -> Result<()> {
    let manifest: Manifest =
        serde_json::from_slice(&std::fs::read(release.join("manifest.json")).context(
            "incomplete Fiber installation; preserve node data and repair this release directory",
        )?)?;
    ensure!(
        digest(&release.join("fnn"))? == manifest.binary_sha256
            && digest(&release.join("config/testnet/config.yml"))? == manifest.config_sha256,
        "installed Fiber files changed; refusing to run them"
    );
    Ok(())
}
pub async fn install() -> Result<PathBuf> {
    ensure!(
        cfg!(all(target_os = "linux", target_arch = "x86_64")),
        "automatic node creation currently supports Linux x86_64; use an existing Fiber 0.9.0 node on other systems"
    );
    let root = service::data_root()?.join("releases");
    service::private_dir(&root)?;
    let release = root.join("fiber-v0.9.0-linux-x86_64");
    if release.exists() {
        verify(&release)?;
        return Ok(release);
    }
    let cache = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .unwrap_or(
            PathBuf::from(std::env::var_os("HOME").context("home directory unavailable")?)
                .join(".cache"),
        )
        .join("liquidlane");
    ensure!(cache.is_absolute(), "cache folder must be absolute");
    service::private_dir(&cache)?;
    let archive = cache.join(ARCHIVE);
    if archive.exists() && digest(&archive)? != DIGEST {
        let invalid = cache.join(format!("{ARCHIVE}.invalid-{}", uuid::Uuid::new_v4()));
        std::fs::rename(&archive, invalid)?;
        println!(
            "An incomplete or altered cached download was set aside. Downloading a verified release again."
        );
    }
    if !archive.exists() {
        println!("Downloading Fiber 0.9.0 from its official release (28 MB)…");
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(300))
            .https_only(true)
            .build()?;
        let mut response = client
            .get(format!(
                "https://github.com/nervosnetwork/fiber/releases/download/v0.9.0/{ARCHIVE}"
            ))
            .send()
            .await?
            .error_for_status()?;
        let temporary = cache.join(format!("{}.download", uuid::Uuid::new_v4()));
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        let mut length = 0usize;
        while let Some(chunk) = response.chunk().await? {
            length += chunk.len();
            ensure!(
                length <= 64 * 1024 * 1024,
                "Fiber release exceeds expected size"
            );
            file.write_all(&chunk)?;
        }
        file.sync_all()?;
        ensure!(
            digest(&temporary)? == DIGEST,
            "downloaded Fiber archive failed its pinned SHA-256 check; nothing was executed"
        );
        std::fs::rename(temporary, &archive)?;
    }
    let staging = root.join(format!(".extract-{}", uuid::Uuid::new_v4()));
    service::private_dir(&staging)?;
    let listing = Command::new("tar").arg("-tzf").arg(&archive).output()?;
    ensure!(
        listing.status.success(),
        "cannot inspect verified Fiber archive"
    );
    let names = String::from_utf8(listing.stdout)?;
    let prefix = if names.lines().any(|n| n == "fnn") {
        ""
    } else {
        "./"
    };
    let output = Command::new("tar")
        .args(["--no-same-owner", "--no-same-permissions", "-xzf"])
        .arg(&archive)
        .arg("-C")
        .arg(&staging)
        .arg(format!("{prefix}fnn"))
        .output()?;
    ensure!(
        output.status.success(),
        "cannot extract verified Fiber release: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let template_folder = staging.join("config/testnet");
    std::fs::create_dir_all(&template_folder)?;
    std::fs::write(
        template_folder.join("config.yml"),
        include_str!("testnet.yml"),
    )?;
    let manifest = Manifest {
        binary_sha256: digest(&staging.join("fnn"))?,
        config_sha256: digest(&staging.join("config/testnet/config.yml"))?,
    };
    super::config::write_private(&staging.join("manifest.json"), &manifest)?;
    std::fs::rename(staging, &release)?;
    println!("Official Fiber archive verified against its pinned SHA-256.");
    Ok(release)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_or_changed_cached_release_is_never_executed() {
        let path =
            std::env::temp_dir().join(format!("liquidlane-release-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(path.join("config/testnet")).unwrap();
        let binary = path.join("fnn");
        let template = path.join("config/testnet/config.yml");
        std::fs::write(&binary, b"test-only executable bytes").unwrap();
        std::fs::write(&template, include_str!("testnet.yml")).unwrap();
        assert!(verify(&path).is_err());
        let manifest = Manifest {
            binary_sha256: digest(&binary).unwrap(),
            config_sha256: digest(&template).unwrap(),
        };
        super::super::config::write_private(&path.join("manifest.json"), &manifest).unwrap();
        verify(&path).unwrap();
        std::fs::write(&binary, b"changed executable bytes").unwrap();
        assert!(verify(&path).is_err());
        std::fs::write(&binary, b"test-only executable bytes").unwrap();
        std::fs::write(&template, b"changed configuration").unwrap();
        assert!(verify(&path).is_err());
        std::fs::remove_dir_all(path).unwrap();
    }
}

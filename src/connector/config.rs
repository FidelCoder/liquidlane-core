use anyhow::{Context, Result, ensure};
use secp256k1::SecretKey;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub core_url: String,
    pub fiber_rpc: String,
    pub ckb_rpc: String,
    pub node_key_file: PathBuf,
    pub state_dir: PathBuf,
    pub node_address: String,
    pub owner_address: String,
    pub max_order_ckb: u64,
    pub max_total_ckb: u64,
    pub min_fee_ckb: u64,
    pub allowed_merchants: Vec<String>,
    #[serde(default)]
    pub accept_public_orders: bool,
    pub auto_approve: bool,
    pub fiber_version: String,
}
impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let config: Self = serde_json::from_slice(&std::fs::read(path)?)?;
        config.validate()?;
        Ok(config)
    }
    pub fn validate(&self) -> Result<()> {
        let config = self;
        ensure!(
            config.fiber_version == "0.9.0",
            "this connector supports Fiber 0.9.0 only"
        );
        let rpc = reqwest::Url::parse(&config.fiber_rpc)?;
        ensure!(
            matches!(rpc.host_str(), Some("localhost" | "127.0.0.1" | "[::1]")),
            "connector requires a loopback Fiber RPC; use a local authenticated tunnel for remote administration"
        );
        let core = reqwest::Url::parse(&config.core_url)?;
        ensure!(
            core.scheme() == "https"
                || matches!(core.host_str(), Some("localhost" | "127.0.0.1" | "[::1]")),
            "marketplace URL must use HTTPS outside localhost"
        );
        ensure!(
            config.max_order_ckb > 0 && config.max_order_ckb <= config.max_total_ckb,
            "explicit positive funding limits required"
        );
        super::super::marketplace::crypto::address_script(&config.owner_address)?;
        Ok(())
    }
    pub fn secret(&self) -> Result<SecretKey> {
        Self::read_secret(&self.node_key_file)
    }
    pub fn run_lock(&self) -> Result<std::fs::File> {
        let lock = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(self.state_dir.join("run.lock"))?;
        lock.try_lock().context(
            "this connector is already running; stop its existing process before starting another",
        )?;
        Ok(lock)
    }
    pub fn read_secret(path: &Path) -> Result<SecretKey> {
        let raw = std::fs::read(path).context("unable to read local node identity key")?;
        let key = if raw.len() == 32 {
            raw
        } else {
            crate::marketplace::crypto::bytes(std::str::from_utf8(&raw)?.trim())?
        };
        Ok(SecretKey::from_slice(&key)?)
    }
}
pub fn write_private(path: &Path, data: &impl Serialize) -> Result<()> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    use std::io::Write;
    let mut file = options.open(&temporary)?;
    file.write_all(serde_json::to_string_pretty(data)?.as_bytes())?;
    file.sync_all()?;
    std::fs::rename(temporary, path)?;
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::File::open(parent)?.sync_all()?;
    }
    Ok(())
}

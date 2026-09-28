use anyhow::{Result, ensure};
use std::{env, path::PathBuf};

#[derive(Clone)]
pub struct Config {
    pub bind: String,
    pub origin: String,
    pub database: PathBuf,
    pub ckb_rpc: String,
    pub fiber_version: String,
}
impl Config {
    pub fn from_env() -> Result<Self> {
        let production = env::var("LIQUIDLANE_ENV").unwrap_or_default() == "production";
        let origin = env::var("LIQUIDLANE_MARKET_ORIGIN")
            .or_else(|_| env::var("LIQUIDLANE_CORS_ALLOWED_ORIGIN"));
        ensure!(
            !production || origin.is_ok(),
            "LIQUIDLANE_MARKET_ORIGIN is required in production"
        );
        let config = Self {
            bind: env::var("LIQUIDLANE_BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:18080".into()),
            origin: origin
                .unwrap_or_else(|_| "http://localhost:3000".into())
                .trim_end_matches('/')
                .into(),
            database: env::var_os("LIQUIDLANE_MARKET_DB")
                .map(PathBuf::from)
                .unwrap_or_else(|| "./liquidlane-marketplace.sqlite3".into()),
            ckb_rpc: env::var("LIQUIDLANE_CKB_RPC_URL")
                .unwrap_or_else(|_| "https://testnet.ckb.dev/rpc".into()),
            fiber_version: env::var("LIQUIDLANE_FIBER_VERSION").unwrap_or_else(|_| "0.9.0".into()),
        };
        let origin_url = reqwest::Url::parse(&config.origin)?;
        ensure!(
            config.fiber_version == "0.9.0",
            "this marketplace supports Fiber 0.9.0 only"
        );
        ensure!(
            !production || origin_url.scheme() == "https",
            "production origin must use HTTPS"
        );
        ensure!(
            origin_url.path() == "/"
                && origin_url.query().is_none()
                && origin_url.fragment().is_none(),
            "origin must not contain a path, query, or fragment"
        );
        Ok(config)
    }
}

use std::{env, net::SocketAddr, path::PathBuf};

#[derive(Clone, Debug)]
pub struct AppConfig {
    pub bind_addr: SocketAddr,
    pub environment: String,
    pub data_path: PathBuf,
    pub fiber_rpc_url: Option<String>,
    pub fiber_rpc_auth_token: Option<String>,
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
        let fiber_rpc_url = env::var("FIBER_RPC_URL")
            .ok()
            .filter(|value| !value.trim().is_empty());
        let fiber_rpc_auth_token = env::var("FIBER_RPC_AUTH_TOKEN")
            .ok()
            .filter(|value| !value.trim().is_empty());

        Ok(Self {
            bind_addr,
            environment,
            data_path,
            fiber_rpc_url,
            fiber_rpc_auth_token,
        })
    }
}

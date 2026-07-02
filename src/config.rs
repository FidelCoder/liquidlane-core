use std::{env, net::SocketAddr};

#[derive(Clone, Debug)]
pub struct AppConfig {
    pub bind_addr: SocketAddr,
    pub environment: String,
}

impl AppConfig {
    pub fn from_env() -> anyhow::Result<Self> {
        let bind_addr = env::var("LIQUIDLANE_BIND_ADDR")
            .unwrap_or_else(|_| "0.0.0.0:8080".to_string())
            .parse()?;

        let environment = env::var("LIQUIDLANE_ENV").unwrap_or_else(|_| "development".to_string());

        Ok(Self {
            bind_addr,
            environment,
        })
    }
}

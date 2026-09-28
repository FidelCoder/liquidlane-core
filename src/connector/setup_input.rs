use anyhow::{Context, Result, ensure};
use serde_json::Value;
use std::{
    io::{self, Write},
    path::PathBuf,
};

pub fn prompt(label: &str, default: Option<&str>) -> Result<String> {
    match default {
        Some(value) => print!("{label} [{value}]: "),
        None => print!("{label}: "),
    }
    io::stdout().flush()?;
    let mut value = String::new();
    ensure!(
        io::stdin().read_line(&mut value)? > 0,
        "setup input ended; run the command in an interactive terminal"
    );
    let value = value.trim();
    let value = if value.is_empty() {
        default.unwrap_or("")
    } else {
        value
    };
    ensure!(!value.is_empty(), "{label} is required");
    Ok(value.into())
}
pub fn local_directory(value: &str) -> Result<PathBuf> {
    let path = if let Some(relative) = value.strip_prefix("~/") {
        PathBuf::from(std::env::var_os("HOME").context("home directory unavailable")?)
            .join(relative)
    } else {
        PathBuf::from(value)
    };
    let path = path
        .canonicalize()
        .context("Fiber data directory does not exist")?;
    ensure!(path.is_dir(), "Fiber data directory must be a folder");
    Ok(path)
}
pub fn tcp_address(info: &Value) -> Option<String> {
    let mut addresses: Vec<_> = info["addresses"]
        .as_array()?
        .iter()
        .filter_map(Value::as_str)
        .filter(|s| {
            s.contains("/tcp/")
                && s.contains("/p2p/")
                && !s.contains("/ws/")
                && !s.contains("/wss/")
        })
        .collect();
    addresses.sort_by_key(|s| s.starts_with("/ip4/127.") || s.starts_with("/ip6/::1/"));
    addresses.first().map(|s| (*s).into())
}

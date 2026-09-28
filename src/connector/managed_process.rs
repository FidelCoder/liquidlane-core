use super::{bootstrap::ManagedNode, release};
use anyhow::{Result, ensure};
use std::{path::Path, process::Command};

fn command(node: &ManagedNode, password: &str) -> Command {
    let mut command = Command::new(node.release.join("fnn"));
    // A desktop/user-manager environment must not override the private RPC, chain,
    // incoming funding policy, key directory, or install an external funding script.
    command.env_clear();
    for name in [
        "HOME",
        "PATH",
        "LANG",
        "RUST_LOG",
        "SSL_CERT_FILE",
        "SSL_CERT_DIR",
        "HTTPS_PROXY",
        "HTTP_PROXY",
        "ALL_PROXY",
        "NO_PROXY",
        "https_proxy",
        "http_proxy",
        "all_proxy",
        "no_proxy",
    ] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    command
        .arg("-d")
        .arg(&node.directory)
        .arg("-c")
        .arg(node.directory.join("config.yml"))
        .env("FIBER_SECRET_KEY_PASSWORD", password);
    command
}

pub fn run(path: &Path) -> Result<()> {
    let node: ManagedNode = serde_json::from_slice(&std::fs::read(path)?)?;
    ensure!(
        node.directory.is_absolute() && node.release.is_absolute(),
        "managed paths must be absolute"
    );
    release::verify(&node.release)?;
    // The unlock secret is never put in command arguments, service files, or sent to Core.
    let password = std::fs::read_to_string(node.directory.join("fiber-password"))?;
    ensure!(
        !password.trim().is_empty(),
        "local Fiber startup credential is empty"
    );
    let mut command = command(&node, password.trim());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        Err(command.exec().into())
    }
    #[cfg(not(unix))]
    {
        anyhow::bail!("managed Fiber services require Linux")
    }
}

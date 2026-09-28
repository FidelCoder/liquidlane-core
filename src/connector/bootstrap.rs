//! Creates a new, unfunded testnet node locally. Existing keys/stores are never replaced.
use super::{config::write_private, release, service};
use crate::marketplace::{chain, crypto, model::TESTNET_GENESIS};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    io::{Read, Write},
    net::{Ipv4Addr, TcpListener, UdpSocket},
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Serialize, Deserialize)]
pub struct ManagedNode {
    pub owner: String,
    pub role: String,
    pub core_url: String,
    pub ckb_rpc: String,
    pub directory: PathBuf,
    pub release: PathBuf,
    pub fiber_rpc: String,
    pub fiber_unit: String,
}
fn create_private(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).context(
        "file already exists; existing node keys and configuration will not be overwritten",
    )?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}
fn random() -> Result<[u8; 32]> {
    let mut bytes = [0; 32];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    Ok(bytes)
}
fn new_key() -> Result<[u8; 32]> {
    loop {
        let key = random()?;
        if secp256k1::SecretKey::from_slice(&key).is_ok() {
            return Ok(key);
        }
    }
}
fn replace(template: &str, old: &str, new: &str) -> Result<String> {
    ensure!(
        template.matches(old).count() == 1,
        "pinned Fiber template no longer matches its expected settings"
    );
    Ok(template.replacen(old, new, 1))
}
pub fn configuration(
    template: &str,
    ckb_rpc: &str,
    rpc_port: u16,
    p2p_port: u16,
    role: &str,
    p2p_ip: Ipv4Addr,
) -> Result<String> {
    let rpc = reqwest::Url::parse(ckb_rpc)?;
    ensure!(
        matches!(rpc.scheme(), "http" | "https"),
        "CKB RPC must use HTTP or HTTPS"
    );
    ensure!(
        rpc_port != p2p_port && rpc_port > 1024 && p2p_port > 1024,
        "distinct unprivileged node ports required"
    );
    ensure!(matches!(role, "provider" | "merchant"), "invalid node role");
    ensure!(
        !p2p_ip.is_unspecified() && !p2p_ip.is_multicast(),
        "usable peer IP required"
    );
    let text = replace(
        template,
        "listening_addr: \"127.0.0.1:8227\"",
        &format!("listening_addr: \"127.0.0.1:{rpc_port}\""),
    )?;
    let text = replace(
        &text,
        "listening_addr: \"/ip4/0.0.0.0/tcp/8228\"",
        &format!("listening_addr: \"/ip4/0.0.0.0/tcp/{p2p_port}\""),
    )?;
    let text = replace(
        &text,
        "rpc_url: \"https://testnet.ckbapp.dev/\"",
        &format!("rpc_url: {}", serde_json::to_string(ckb_rpc)?),
    )?;
    let text = replace(
        &text,
        "  announced_addrs:",
        &format!("  announced_addrs:\n    - \"/ip4/{p2p_ip}/tcp/{p2p_port}\""),
    )?;
    // Public peers cannot spend a provider's reserve through automatic incoming openings.
    // Outgoing marketplace openings are still authorized by the connector's local policy.
    let minimum = if role == "provider" {
        u64::MAX
    } else {
        10_000_000_000
    };
    let reserve = if role == "provider" {
        0
    } else {
        9_900_000_000u64
    };
    replace(
        &text,
        "  chain: testnet",
        &format!(
            "  chain: testnet\n  open_channel_auto_accept_min_ckb_funding_amount: {minimum}\n  auto_accept_channel_ckb_funding_amount: {reserve}"
        ),
    )
}
pub async fn create(owner: &str, role: &str, core_url: &str, ckb_rpc: &str) -> Result<ManagedNode> {
    service::available()?;
    crypto::address_script(owner)?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    chain::verify_network(&client, ckb_rpc).await?;
    let nodes = service::data_root()?.join("nodes");
    service::private_dir(&nodes)?;
    let directory = nodes.join(format!(
        "{}-{role}",
        &crypto::digest(owner.as_bytes())[..24]
    ));
    let metadata = directory.join("managed-node.json");
    let managed = if metadata.exists() {
        let node: ManagedNode = serde_json::from_slice(&std::fs::read(&metadata)?)?;
        ensure!(
            node.owner == owner
                && node.role == role
                && node.core_url == core_url
                && node.ckb_rpc == ckb_rpc
                && node.directory == directory,
            "this local node belongs to another setup; existing keys and policy are preserved"
        );
        ensure!(
            directory.join("ckb/key").is_file() && directory.join("fiber-password").is_file(),
            "node key or startup credential missing; restore the original files, never replace a funded node"
        );
        release::verify(&node.release)?;
        node
    } else {
        ensure!(
            !directory.exists(),
            "incomplete node directory already exists; inspect it before setup, no keys were replaced"
        );
        let release = release::install().await?;
        service::private_dir(&directory)?;
        service::private_dir(&directory.join("ckb"))?;
        let rpc = TcpListener::bind("127.0.0.1:0")?;
        let p2p = TcpListener::bind("127.0.0.1:0")?;
        let rpc_port = rpc.local_addr()?.port();
        let p2p_port = p2p.local_addr()?.port();
        let template = std::fs::read_to_string(release.join("config/testnet/config.yml"))?;
        // Route lookup sends no packet. Advertise this machine's actual interface,
        // never the wildcard listen address. NAT/public reachability is separate.
        let route = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))?;
        route.connect(("54.179.226.154", 8228))?;
        let std::net::IpAddr::V4(peer_ip) = route.local_addr()?.ip() else {
            anyhow::bail!("an IPv4 peer interface is required by this testnet setup");
        };
        let configuration = configuration(&template, ckb_rpc, rpc_port, p2p_port, role, peer_ip)?;
        create_private(
            &directory.join("ckb/key"),
            hex::encode(new_key()?).as_bytes(),
        )?;
        create_private(
            &directory.join("fiber-password"),
            hex::encode(random()?).as_bytes(),
        )?;
        create_private(&directory.join("config.yml"), configuration.as_bytes())?;
        let node = ManagedNode {
            owner: owner.into(),
            role: role.into(),
            core_url: core_url.into(),
            ckb_rpc: ckb_rpc.into(),
            release,
            fiber_rpc: format!("http://127.0.0.1:{rpc_port}"),
            fiber_unit: service::unit_name("fiber", &directory)?,
            directory: directory.clone(),
        };
        write_private(&metadata, &node)?;
        node
    };
    let executable = std::env::current_exe()?.canonicalize()?;
    let body = service::definition(
        "LiquidLane local Fiber testnet node",
        &executable,
        &[
            "fiber-run",
            metadata.to_str().context("metadata path must be UTF-8")?,
        ],
        &directory,
    )?;
    service::install_unit(&managed.fiber_unit, &body)?;
    println!("Fiber runs in the background. Waiting for its verified testnet identity…");
    let mut last_error = String::new();
    for _ in 0..30 {
        match chain::rpc(&client, &managed.fiber_rpc, "node_info", json!([])).await {
            Ok(info) => {
                ensure!(
                    info["version"] == "0.9.0" && info["chain_hash"] == TESTNET_GENESIS,
                    "managed node version or network mismatch"
                );
                let key = super::config::Config::read_secret(&directory.join("fiber/sk"))?;
                ensure!(
                    info["pubkey"] == crypto::pubkey(&key),
                    "managed node RPC identity does not match its key"
                );
                if role == "provider" {
                    ensure!(
                        chain::number(&info["auto_accept_channel_ckb_funding_amount"])? == 0,
                        "provider node must disable automatic incoming channel funding"
                    );
                }
                println!(
                    "Node ready. CKB funding address: {}",
                    crypto::funding_address(&info["default_funding_lock_script"])?
                );
                return Ok(managed);
            }
            Err(error) => last_error = error.to_string(),
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    anyhow::bail!(
        "Fiber has not become ready: {last_error}. Inspect journalctl --user -u {}. Node files preserved in {}",
        managed.fiber_unit,
        directory.display()
    )
}
#[cfg(test)]
#[path = "bootstrap_tests.rs"]
mod tests;

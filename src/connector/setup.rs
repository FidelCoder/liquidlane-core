use super::{
    Connector, cli,
    config::{Config, write_private},
    setup_input::{local_directory, prompt, tcp_address},
};
use crate::marketplace::{
    chain, crypto,
    model::{TESTNET_GENESIS, now},
    nodes,
};
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use serde_json::json;
use std::{path::Path, time::Duration};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PairingFile {
    format: String,
    core_url: String,
    ckb_rpc: String,
    owner_address: String,
    pairing_code: String,
    expires_at: i64,
    role: String,
    provider_policy: Option<ProviderPolicy>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProviderPolicy {
    max_order_ckb: u64,
    max_total_ckb: u64,
    min_fee_ckb: u64,
    #[serde(default)]
    allowed_merchants: Vec<String>,
    #[serde(default)]
    accept_public_orders: bool,
    #[serde(default)]
    auto_approve: bool,
}

pub async fn setup(path: &Path, once: bool, background: bool, new_node: bool) -> Result<()> {
    ensure!(
        !(once && background),
        "choose either --once or --background"
    );
    ensure!(
        !new_node || background,
        "--new-node requires --background so Fiber and its connector keep running"
    );
    if background {
        super::service::available()?;
    }
    let pairing: PairingFile = serde_json::from_slice(
        &std::fs::read(path)
            .context("download the pairing file from your node setup page first")?,
    )?;
    ensure!(
        pairing.format == "liquidlane-pairing/1",
        "unsupported pairing file"
    );
    ensure!(
        pairing.expires_at > now(),
        "pairing code expired; create a new code in your node workspace and download its pairing file"
    );
    ensure!(
        matches!(pairing.role.as_str(), "merchant" | "provider"),
        "invalid node role"
    );
    ensure!(
        crypto::bytes(&pairing.pairing_code)?.len() == 32,
        "invalid pairing code"
    );
    println!("Pairing a {} node on CKB testnet.", pairing.role);
    let policy = match (pairing.role.as_str(), pairing.provider_policy) {
        ("provider", Some(policy)) => {
            ensure!(
                policy.accept_public_orders || !policy.allowed_merchants.is_empty(),
                "enable accept_public_orders to serve the marketplace, or supply a restricted merchant list"
            );
            for address in &policy.allowed_merchants {
                crypto::address_script(address)?;
            }
            policy
        }
        ("provider", None) => {
            anyhow::bail!("provider funding policy missing; download a new pairing file")
        }
        // These limits are checked only for the provider role; a merchant never opens an order.
        _ => ProviderPolicy {
            max_order_ckb: 1,
            max_total_ckb: 1,
            min_fee_ckb: 0,
            allowed_merchants: vec![],
            accept_public_orders: false,
            auto_approve: false,
        },
    };
    let managed = if new_node {
        let core = reqwest::Url::parse(&pairing.core_url)?;
        ensure!(
            core.scheme() == "https"
                || matches!(core.host_str(), Some("localhost" | "127.0.0.1" | "[::1]")),
            "marketplace URL must use HTTPS outside localhost"
        );
        ensure!(
            policy.max_order_ckb > 0 && policy.max_order_ckb <= policy.max_total_ckb,
            "explicit positive funding limits required"
        );
        Some(
            super::bootstrap::create(
                &pairing.owner_address,
                &pairing.role,
                &pairing.core_url,
                &pairing.ckb_rpc,
            )
            .await?,
        )
    } else {
        None
    };
    let (fiber_rpc, directory) = if let Some(node) = &managed {
        (node.fiber_rpc.clone(), node.directory.clone())
    } else {
        println!(
            "Use the RPC URL and data directory from your running Fiber node's configuration."
        );
        let rpc = prompt("Local Fiber RPC URL", Some("http://127.0.0.1:8227"))?;
        let directory =
            local_directory(&prompt("Fiber data directory (contains fiber/sk)", None)?)?;
        (rpc, directory)
    };
    let node_key_file = directory
        .join("fiber/sk")
        .canonicalize()
        .context("fiber/sk was not found; use your existing Fiber data directory")?;
    let connector_directory = managed
        .as_ref()
        .map(|node| node.directory.clone())
        .unwrap_or(std::env::current_dir()?);
    let mut config = Config {
        core_url: pairing.core_url,
        fiber_rpc,
        ckb_rpc: pairing.ckb_rpc,
        node_key_file,
        state_dir: connector_directory.join("connector-state"),
        node_address: String::new(),
        owner_address: pairing.owner_address,
        max_order_ckb: policy.max_order_ckb,
        max_total_ckb: policy.max_total_ckb,
        min_fee_ckb: policy.min_fee_ckb,
        allowed_merchants: policy.allowed_merchants,
        accept_public_orders: policy.accept_public_orders,
        auto_approve: policy.auto_approve,
        fiber_version: "0.9.0".into(),
    };
    config.validate()?;
    if pairing.role == "provider" && config.accept_public_orders && config.auto_approve {
        println!(
            "Automatic marketplace: any merchant may accept your published offer. The connector opens eligible channels within your {} CKB per-order and {} CKB total limits. Fees are collected after delivery and can remain unpaid.",
            config.max_order_ckb, config.max_total_ckb
        );
    }
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let info = chain::rpc(&client, &config.fiber_rpc, "node_info", json!([])).await
        .context("cannot read node_info; check that Fiber is running and the RPC URL matches its configuration")?;
    ensure!(
        info["version"] == config.fiber_version && info["chain_hash"] == TESTNET_GENESIS,
        "use a running Fiber 0.9.0 node on CKB testnet"
    );
    let pubkey = crypto::pubkey(&config.secret()?);
    ensure!(
        info["pubkey"] == pubkey,
        "that data directory belongs to a different node than the RPC URL"
    );
    if pairing.role == "merchant" {
        ensure!(
            chain::number(&info["auto_accept_channel_ckb_funding_amount"])? >= 99 * 100_000_000,
            "receiving node must auto-fund at least its 99 CKB reserve; update Fiber's auto_accept_channel_ckb_funding_amount and restart Fiber before pairing"
        );
    }
    let address = tcp_address(&info);
    println!("Node identity verified. Its existing identity key stays on this machine.");
    println!("Node identity: {pubkey}");
    println!(
        "Node funding address: {}",
        crypto::funding_address(&info["default_funding_lock_script"])?
    );
    if pairing.role == "provider" {
        println!(
            "After pairing, compare this address with the website's Add capital step. Sending CKB there funds this node's wallet."
        );
    } else {
        println!(
            "This node's wallet funds its channel reserve. Your connected browser wallet pays the opening fee separately."
        );
    }
    println!(
        "The provider must be able to reach this TCP address. Keep the detected address for peers on the same machine; otherwise enter your node's reachable address."
    );
    config.node_address = if new_node {
        address.context("managed node did not report a TCP address")?
    } else {
        prompt("Fiber TCP /p2p/ address", address.as_deref())?
    };
    nodes::validate_node(&pubkey, &config.node_address)?;
    let config_path = connector_directory.join("connector.json");
    if config_path.exists() {
        let previous = Config::load(&config_path)?;
        ensure!(
            previous.owner_address == config.owner_address
                && crypto::pubkey(&previous.secret()?) == pubkey,
            "connector.json belongs to another wallet or node; run setup from a separate connector folder"
        );
        config.state_dir = previous.state_dir;
    } else {
        ensure!(
            !config.state_dir.exists(),
            "connector-state already exists without its configuration; restore that configuration or use a separate connector folder"
        );
    }
    super::service::private_dir(&config.state_dir)?;
    let setup_lock = if background {
        super::service::preflight(&config)?
    } else {
        Some(config.run_lock()?)
    };
    write_private(&config_path, &config)?;
    cli::pair(config.clone(), &pairing.pairing_code).await?;
    drop(setup_lock);
    if background {
        super::service::install(&config_path)?;
        println!(
            "Return to Supply Liquidity. Once the live check-in arrives, add CKB from your wallet to the verified node address."
        );
        return Ok(());
    }
    println!(
        "Saved {}. Starting the connector; the website confirms the connection after its first heartbeat.",
        config_path.display()
    );
    println!(
        "Keep this process running. To resume later from this folder: liquidlane-connector run ./connector.json"
    );
    Connector::load(config)?.run(once).await
}

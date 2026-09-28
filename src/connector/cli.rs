use super::{
    Connector, Registration,
    config::{Config, write_private},
};
use crate::marketplace::{
    chain, crypto,
    model::{SHANNONS, TESTNET_GENESIS},
    nodes,
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{path::Path, time::Duration};

pub async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let args: Vec<String> = std::env::args().skip(1).collect();
    ensure!(
        args.len() >= 2,
        "usage: liquidlane-connector setup <pairing.json> [--once|--background] [--new-node], or <pair|run|approve|status|node-status|settlement-status|pay-invoice|payment-status|close-channel> <config.json> [code|order|invoice|hash|channel] [--once|--force], or <service-install|service-status|service-start|service-stop> <config.json>"
    );
    if args[0] == "setup" {
        ensure!(
            args.iter()
                .skip(2)
                .all(|s| matches!(s.as_str(), "--once" | "--background" | "--new-node")),
            "unknown setup option"
        );
        return super::setup::setup(
            Path::new(&args[1]),
            args.iter().any(|s| s == "--once"),
            args.iter().any(|s| s == "--background"),
            args.iter().any(|s| s == "--new-node"),
        )
        .await;
    }
    if args[0] == "fiber-run" {
        return super::managed_process::run(Path::new(&args[1]));
    }
    if args[0] == "service-install" {
        super::service::install(Path::new(&args[1]))?;
        return Ok(());
    }
    if matches!(
        args[0].as_str(),
        "service-status" | "service-start" | "service-stop"
    ) {
        return super::service::action(Path::new(&args[1]), &args[0]);
    }
    let config = Config::load(Path::new(&args[1]))?;
    let arg = || args.get(2).context("command requires one more argument");
    match args[0].as_str() {
        "pair" => pair(config, arg()?).await,
        "node-status" => {
            println!(
                "{}",
                serde_json::to_string_pretty(&node_info(&config).await?)?
            );
            Ok(())
        }
        "pay-invoice" => pay_invoice(&config, arg()?).await,
        "payment-status" => {
            println!(
                "{}",
                rpc(&config, "get_payment", json!([{"payment_hash":arg()?}])).await?
            );
            Ok(())
        }
        "close-channel" => close(&config, arg()?, args.iter().any(|s| s == "--force")).await,
        "settlement-status" => {
            let client = reqwest::Client::builder()
                .timeout(Duration::from_secs(25))
                .build()?;
            let proof =
                crate::marketplace::settlement::inspect(&client, &config.ckb_rpc, arg()?).await?;
            println!("{}", serde_json::to_string_pretty(&proof)?);
            Ok(())
        }
        "run" => {
            Connector::load(config)?
                .run(args.iter().any(|s| s == "--once"))
                .await
        }
        "approve" => Connector::load(config)?.approve(arg()?).await,
        "status" => Connector::load(config)?.inspect().await,
        _ => anyhow::bail!("unknown connector command"),
    }
}

async fn rpc(config: &Config, method: &str, params: Value) -> Result<Value> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    chain::rpc(&client, &config.fiber_rpc, method, params).await
}
async fn node_info(config: &Config) -> Result<Value> {
    let info = rpc(config, "node_info", json!([])).await?;
    ensure!(
        info["pubkey"] == crypto::pubkey(&config.secret()?),
        "local node identity differs from configured key"
    );
    ensure!(
        info["version"] == config.fiber_version && info["chain_hash"] == TESTNET_GENESIS,
        "node must match the testnet version pin"
    );
    Ok(info)
}
pub(super) async fn pair(config: Config, code: &str) -> Result<()> {
    let info = node_info(&config).await?;
    let pubkey = info["pubkey"].as_str().context("node pubkey missing")?;
    nodes::validate_node(pubkey, &config.node_address)?;
    let message = nodes::registration_message(code, pubkey, &config.node_address);
    let input = nodes::Registration {
        pairing_code: code.into(),
        pubkey: pubkey.into(),
        address: config.node_address.clone(),
        version: config.fiber_version.clone(),
        chain_hash: TESTNET_GENESIS.into(),
        signature: crypto::sign(&config.secret()?, &message),
    };
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let response = client
        .post(format!(
            "{}/connector/register",
            config.core_url.trim_end_matches('/')
        ))
        .json(&input)
        .send()
        .await?;
    let status = response.status();
    let value: Value = response.json().await?;
    ensure!(status.is_success(), "pairing failed: {}", value["error"]);
    let registration: Registration = serde_json::from_value(value)?;
    ensure!(
        registration.node.owner == config.owner_address,
        "pairing account differs from local owner policy"
    );
    write_private(&config.state_dir.join("registration.json"), &registration)?;
    println!(
        "Paired {} node {}. Credential stored locally with owner-only permissions.",
        registration.node.role, registration.node.id
    );
    Ok(())
}
async fn pay_invoice(config: &Config, invoice: &str) -> Result<()> {
    node_info(config).await?;
    let parsed = rpc(config, "parse_invoice", json!([{"invoice":invoice}])).await?;
    ensure!(
        parsed["invoice"]["currency"] == "Fibt"
            && chain::number(&parsed["invoice"]["amount"])? == SHANNONS,
        "only a 1 CKB testnet delivery probe is allowed by this command"
    );
    let hash = parsed["invoice"]["data"]["payment_hash"]
        .as_str()
        .context("invoice hash missing")?;
    let path = config
        .state_dir
        .join(format!("payment-{}.json", crypto::digest(hash.as_bytes())));
    if path.exists() {
        let previous = rpc(config, "get_payment", json!([{"payment_hash":hash}])).await?;
        ensure!(
            previous["status"] == "Failed",
            "payment already attempted; use payment-status to reconcile it"
        );
        // v0.9.0 also refuses failed sessions whose attempts remain in flight.
    }
    write_private(
        &path,
        &json!({"payment_hash":hash,"invoice":invoice,"state":"submission_pending"}),
    )?;
    let result = rpc(
        config,
        "send_payment",
        json!([{"invoice":invoice,"max_fee_amount":"0xf4240","timeout":"0x78","max_parts":"0x1"}]),
    )
    .await?;
    write_private(&path, &result)?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}
async fn close(config: &Config, id: &str, force: bool) -> Result<()> {
    let info = node_info(config).await?;
    ensure!(crypto::bytes(id)?.len() == 32, "valid channel ID required");
    let channels = rpc(config, "list_channels", json!([{"include_closed":true}])).await?;
    let channel = channels["channels"]
        .as_array()
        .context("channel list missing")?
        .iter()
        .find(|c| c["channel_id"] == id)
        .context("channel not found on this local node")?;
    ensure!(
        super::execution::channel_state(channel) != "Closed",
        "channel already reports Closed; inspect settlement separately"
    );
    let params = if force {
        json!([{"channel_id":id,"force":true}])
    } else {
        json!([{"channel_id":id,"force":false,"close_script":info["default_funding_lock_script"],"fee_rate":"0x7d0"}])
    };
    let result = rpc(config, "shutdown_channel", params).await?;
    println!(
        "Shutdown submitted: {result}. Closure and spendable settlement are separate observations."
    );
    Ok(())
}

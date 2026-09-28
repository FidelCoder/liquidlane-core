//! Explicit local commands for native CKB testnet wallets. Never runs in the server.
mod transfer;
use crate::{
    connector::config::write_private,
    marketplace::{crypto, joyid::Proof, model::Order},
};
use anyhow::{Context, Result, ensure};
use secp256k1::SecretKey;
use serde_json::{Value, json};
use std::path::Path;

fn secret(path: &str) -> Result<SecretKey> {
    let bytes = std::fs::read(path).context("cannot read local wallet key file")?;
    let bytes = if bytes.len() == 32 {
        bytes
    } else {
        crypto::bytes(std::str::from_utf8(&bytes)?.trim())?
    };
    Ok(SecretKey::from_slice(&bytes)?)
}
fn proof(secret: &SecretKey, message: &str) -> Result<Proof> {
    let pubkey = crypto::pubkey(secret);
    Ok(Proof {
        scheme: "ckb_secp256k1".into(),
        address: crypto::native_address(&pubkey)?,
        pubkey,
        signature: crypto::sign(secret, message),
        message: String::new(),
        challenge: String::new(),
        key_type: String::new(),
        alg: 0,
    })
}
pub async fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    ensure!(
        args.len() >= 2,
        "usage: liquidlane-wallet address KEY_FILE | login KEY_FILE CORE_URL SESSION_FILE | approve KEY_FILE CORE_URL SESSION_FILE ORDER_ID | transfer KEY_FILE CKB_RPC RECIPIENT CKB RECEIPT_FILE"
    );
    let key = secret(&args[1])?;
    let address = crypto::native_address(&crypto::pubkey(&key))?;
    match args[0].as_str() {
        "address" => println!("{address}"),
        "login" => {
            ensure!(
                args.len() == 4,
                "login requires key file, core URL, and session file"
            );
            let challenge = request(
                &args[2],
                "/auth/challenge",
                None,
                Some(json!({"address":address})),
            )
            .await?;
            let message = challenge["message"].as_str().context("missing challenge")?;
            ensure!(
                message.starts_with("LiquidLane wallet session\n")
                    && message.contains(&format!("\nAddress: {address}\n")),
                "unexpected login challenge"
            );
            let response = request(
                &args[2],
                "/auth/verify",
                None,
                Some(
                    json!({"challenge_id":challenge["challenge_id"],"proof":proof(&key,message)?}),
                ),
            )
            .await?;
            write_private(Path::new(&args[3]), &response)?;
            println!("Authenticated {address}; session stored in {}", args[3]);
        }
        "approve" => {
            ensure!(
                args.len() == 5,
                "approve requires key file, core URL, session file, and order ID"
            );
            let session: Value = serde_json::from_slice(&std::fs::read(&args[3])?)?;
            let token = session["token"].as_str().context("session token missing")?;
            let dashboard = request(&args[2], "/market/dashboard", Some(token), None).await?;
            let order: Order = serde_json::from_value(
                dashboard["orders"]
                    .as_array()
                    .context("orders missing")?
                    .iter()
                    .find(|o| o["id"] == args[4])
                    .context("order not found")?
                    .clone(),
            )?;
            ensure!(order.owner == address, "wallet does not own this order");
            let message = crypto::quote_message(&order.quote)?;
            ensure!(
                crypto::digest(message.as_bytes()) == order.quote_hash,
                "altered quote"
            );
            crypto::verify(
                &order.quote.provider_pubkey,
                &message,
                order
                    .provider_signature
                    .as_deref()
                    .context("provider has not signed")?,
            )?;
            let result = request(
                &args[2],
                &format!("/market/orders/{}/accept", order.id),
                Some(token),
                Some(json!({"proof":proof(&key,&order.approval_message())?})),
            )
            .await?;
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        "transfer" => {
            ensure!(
                args.len() == 6,
                "transfer requires key file, CKB RPC, recipient, whole CKB amount, and receipt file"
            );
            let rpc = args[2].clone();
            let recipient = args[3].clone();
            let amount = args[4].parse::<u64>()?;
            let receipt = args[5].clone();
            tokio::task::spawn_blocking(move || {
                transfer::send(&key, &rpc, &recipient, amount, Path::new(&receipt))
            })
            .await??;
        }
        _ => anyhow::bail!("unknown wallet command"),
    }
    Ok(())
}
async fn request(
    core: &str,
    path: &str,
    token: Option<&str>,
    body: Option<Value>,
) -> Result<Value> {
    let url = reqwest::Url::parse(core)?;
    ensure!(
        url.scheme() == "https"
            || matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]")),
        "use HTTPS for remote wallets"
    );
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let url = format!("{}{path}", core.trim_end_matches('/'));
    let request = if let Some(body) = body {
        client.post(url).json(&body)
    } else {
        client.get(url)
    };
    let request = if let Some(token) = token {
        request.bearer_auth(token)
    } else {
        request
    };
    let response = request.send().await?;
    let status = response.status();
    let value: Value = response.json().await?;
    ensure!(status.is_success(), "{}", value["error"]);
    Ok(value)
}

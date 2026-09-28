mod bootstrap;
mod budget;
pub mod cli;
pub mod config;
mod execution;
mod managed_process;
mod observe;
mod policy;
mod release;
mod rpc;
mod service;
mod settlement;
mod setup;
mod setup_input;
mod worker;

use crate::marketplace::{
    crypto,
    db::Db,
    model::{Node, now},
    nodes::SignedPayload,
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::time::Duration;

#[derive(Serialize, Deserialize)]
pub struct Registration {
    pub token: String,
    pub node: Node,
    pub expires_at: i64,
}
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Journal {
    pub order_id: String,
    pub quote_hash: String,
    pub approved: bool,
    pub attempted: bool,
    pub baseline_channels: Vec<String>,
    pub channel_id: Option<String>,
    pub funding_outpoint: Option<String>,
    pub invoice: Option<Value>,
    pub error: Option<String>,
    pub started_at: i64,
    #[serde(default)]
    pub funding_ckb: u64,
}
pub struct Connector {
    pub config: config::Config,
    pub registration: Registration,
    pub client: reqwest::Client,
    pub db: Db,
    secret: secp256k1::SecretKey,
}
impl Connector {
    pub fn load(config: config::Config) -> Result<Self> {
        let registration =
            serde_json::from_slice(&std::fs::read(config.state_dir.join("registration.json"))?)?;
        let secret = config.secret()?;
        let db = Db::open(&config.state_dir.join("connector.sqlite3"))?;
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        let connector = Self {
            config,
            registration,
            client,
            db,
            secret,
        };
        ensure!(
            connector.registration.node.pubkey == crypto::pubkey(&connector.secret),
            "paired node identity does not match local key"
        );
        ensure!(
            connector.registration.node.owner == connector.config.owner_address,
            "paired wallet differs from local policy"
        );
        Ok(connector)
    }
    pub fn signed(&self, mut value: Value) -> Result<SignedPayload> {
        value["node_id"] = json!(self.registration.node.id);
        value["at"] = json!(now());
        let payload = serde_json::to_string(&value)?;
        Ok(SignedPayload {
            signature: crypto::sign(&self.secret, &payload),
            payload,
        })
    }
    pub async fn core(&self, path: &str, body: Option<Value>) -> Result<Value> {
        let url = format!("{}{}", self.config.core_url.trim_end_matches('/'), path);
        let request = if let Some(body) = body {
            self.client.post(url).json(&body)
        } else {
            self.client.get(url)
        };
        let response = request.bearer_auth(&self.registration.token).send().await?;
        let status = response.status();
        let value: Value = response.json().await?;
        ensure!(
            status.is_success(),
            "marketplace: {}",
            value["error"].as_str().unwrap_or("request failed")
        );
        Ok(value)
    }
    pub async fn post_signed(&self, path: &str, value: Value) -> Result<Value> {
        self.core(path, Some(serde_json::to_value(self.signed(value)?)?))
            .await
    }
    pub fn journal(&self, id: &str) -> Result<Journal> {
        Ok(self
            .db
            .list::<Journal>("journal")?
            .into_iter()
            .find(|j| j.order_id == id)
            .unwrap_or_else(|| Journal {
                order_id: id.into(),
                ..Default::default()
            }))
    }
    pub fn save(&self, journal: &Journal) -> Result<()> {
        self.db.transaction(|tx| {
            crate::marketplace::db::put(tx, "journal", &journal.order_id, "local", journal)
        })
    }
    pub fn run_lock(&self) -> Result<std::fs::File> {
        self.config.run_lock()
    }
    pub async fn run(&self, once: bool) -> Result<()> {
        let _run_lock = self.run_lock()?;
        self.node_info().await?;
        if once {
            return self.tick().await;
        }
        loop {
            match self.tick().await {
                Ok(()) => {}
                Err(error) => tracing::warn!(%error,"connector cycle incomplete"),
            }
            tokio::select! {_=tokio::signal::ctrl_c()=>break,_=tokio::time::sleep(Duration::from_secs(8))=>{}}
        }
        Ok(())
    }
    pub async fn approve(&self, id: &str) -> Result<()> {
        let orders = self.core("/connector/orders", None).await?;
        let order: crate::marketplace::model::Order = serde_json::from_value(
            orders["orders"]
                .as_array()
                .context("missing orders")?
                .iter()
                .find(|o| o["id"] == id)
                .context("order not found")?
                .clone(),
        )?;
        self.validate_order(&order, true).await?;
        let mut journal = self.journal(id)?;
        journal.quote_hash = order.quote_hash;
        journal.approved = true;
        self.save(&journal)?;
        println!("Approved order {id} under the local funding limits.");
        Ok(())
    }
}

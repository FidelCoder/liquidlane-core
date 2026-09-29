#![allow(dead_code)]
use axum::{Json, Router, routing::post};
use liquidlane_core::marketplace::{chain, model::TESTNET_GENESIS};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

pub const FIRST: &str = "0xce614be959f38db1d4f640b3fda89e5c5a88966199d1145d077be77d9f68d244#0";
pub const SECOND: &str = "0x2650ce1864b810ba5a4809f32edd5fb6db55fd57b0b8c53d7910a5b15cae1f2e#0";

pub fn records() -> BTreeMap<String, Value> {
    let mut records: BTreeMap<String, Value> =
        serde_json::from_str(include_str!("../fixtures/recovery-funding.json")).unwrap();
    for text in [
        include_str!("../../docs/evidence/coordinator-outage/forced-commitments.json"),
        include_str!("../../docs/evidence/coordinator-outage/forced-settlements.json"),
    ] {
        let data: Value = serde_json::from_str(text).unwrap();
        for (hash, tx) in data["transactions"].as_object().unwrap() {
            records.insert(hash.clone(), tx.clone());
        }
    }
    records
}

pub struct Rpc {
    pub url: String,
    pub requests: Arc<Mutex<Vec<Value>>>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Rpc {
    fn drop(&mut self) {
        self.task.abort();
    }
}
pub async fn serve(records: BTreeMap<String, Value>, empty_indexer: bool) -> Rpc {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let log = requests.clone();
    let app = Router::new().route(
        "/",
        post(move |Json(request): Json<Value>| {
            let records = records.clone();
            let log = log.clone();
            async move {
                log.lock().unwrap().push(request.clone());
                let result = match request["method"].as_str().unwrap() {
                    "get_block_hash" => json!(TESTNET_GENESIS),
                    "get_transaction" => records
                        .get(request["params"][0].as_str().unwrap())
                        .cloned()
                        .unwrap_or(Value::Null),
                    "get_live_cell" => json!({"status":"unknown","cell":null}),
                    "get_transactions" => {
                        let lock = &request["params"][0]["script"];
                        let mut objects = Vec::new();
                        if !empty_indexer {
                            for (hash, record) in &records {
                                for input in record["transaction"]["inputs"].as_array().unwrap() {
                                    let point = &input["previous_output"];
                                    if let Some(parent) =
                                        records.get(point["tx_hash"].as_str().unwrap())
                                    {
                                        let index =
                                            chain::number(&point["index"]).unwrap() as usize;
                                        if &parent["transaction"]["outputs"][index]["lock"] == lock
                                        {
                                            objects.push(json!({"tx_hash":hash,"io_type":"input"}));
                                        }
                                    }
                                }
                            }
                        }
                        json!({"objects":objects,"last_cursor":"0x01"})
                    }
                    method => panic!("unexpected RPC {method}"),
                };
                Json(json!({"jsonrpc":"2.0","id":request["id"],"result":result}))
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    Rpc {
        url,
        requests,
        task,
    }
}

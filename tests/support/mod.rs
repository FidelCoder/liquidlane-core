#![allow(dead_code)]
use axum::{Router, body::Body, http::Request};
use http_body_util::BodyExt;
use liquidlane_core::marketplace::{
    Market,
    config::Config,
    crypto, db,
    joyid::Proof,
    model::{Node, Offer, Order, now},
    nodes::SignedPayload,
};
use secp256k1::SecretKey;
use serde_json::{Value, json};
use tower::ServiceExt;

pub fn key(n: u8) -> SecretKey {
    SecretKey::from_slice(&[n; 32]).unwrap()
}
pub fn address(n: u8) -> String {
    crypto::native_address(&crypto::pubkey(&key(n))).unwrap()
}
pub fn market() -> Market {
    Market::new(Config {
        bind: "127.0.0.1:0".into(),
        origin: "http://localhost:3000".into(),
        database: ":memory:".into(),
        ckb_rpc: "http://127.0.0.1:1".into(),
        fiber_version: "0.9.0".into(),
    })
    .unwrap()
}
pub fn proof(n: u8, message: &str) -> Proof {
    Proof {
        scheme: "ckb_secp256k1".into(),
        address: address(n),
        pubkey: crypto::pubkey(&key(n)),
        signature: crypto::sign(&key(n), message),
        message: String::new(),
        challenge: String::new(),
        key_type: String::new(),
        alg: 0,
    }
}
pub async fn request(
    app: &Router,
    path: &str,
    token: Option<&str>,
    body: Option<Value>,
) -> (u16, Value) {
    let mut request =
        Request::builder()
            .uri(path)
            .method(if body.is_some() { "POST" } else { "GET" });
    if let Some(token) = token {
        request = request.header("authorization", format!("Bearer {token}"));
    }
    let request = request
        .header("content-type", "application/json")
        .body(Body::from(body.map(|b| b.to_string()).unwrap_or_default()))
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status().as_u16();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(json!(String::from_utf8_lossy(&bytes))),
    )
}
pub fn node(id: &str, n: u8, role: &str, capacity: u64) -> Node {
    Node {
        id: id.into(),
        owner: address(n),
        role: role.into(),
        label: id.into(),
        pubkey: crypto::pubkey(&key(n)),
        address: format!("/ip4/127.0.0.1/tcp/8228/p2p/test-{id}"),
        version: "0.9.0".into(),
        last_seen: now(),
        available_ckb: capacity,
        reserve_ckb: 99,
        provider_policy: None,
        funding: None,
        background: false,
    }
}
pub fn seed(state: &Market, capacity: u64) {
    state
        .db
        .transaction(|tx| {
            for node in [
                node("provider", 1, "provider", capacity),
                node("merchant", 2, "merchant", 1000),
                node("second", 3, "merchant", 1000),
            ] {
                db::put(tx, "node", &node.id, &node.owner, &node)?;
                for (token, scope, actor) in [
                    (
                        format!("{}-account", node.id),
                        "account",
                        node.owner.clone(),
                    ),
                    (format!("{}-node", node.id), "node", node.id.clone()),
                ] {
                    tx.execute(
                        "INSERT INTO sessions VALUES(?1,?2,?3,?4)",
                        rusqlite::params![
                            crypto::digest(token.as_bytes()),
                            actor,
                            scope,
                            now() + 3600
                        ],
                    )?;
                }
            }
            let offer = Offer {
                id: "offer".into(),
                provider_node: "provider".into(),
                owner: address(1),
                min_capacity_ckb: 100,
                max_capacity_ckb: 2000,
                opening_fee_ckb: 61,
                public_channel: true,
                enabled: true,
                created_at: now(),
                expires_at: now() + 86400,
            };
            db::put(tx, "offer", "offer", &offer.owner, &offer)
        })
        .unwrap();
}
pub fn order_body(merchant: &str, idempotency: &str) -> Value {
    json!({"offer_id":"offer","merchant_node":merchant,"capacity_ckb":500,"idempotency_key":idempotency})
}
pub async fn create(app: &Router) -> Order {
    let (status, value) = request(
        app,
        "/market/orders",
        Some("merchant-account"),
        Some(order_body("merchant", "unique-order-request")),
    )
    .await;
    assert_eq!(status, 200, "{value}");
    serde_json::from_value(value).unwrap()
}
pub fn signed(node: &Node, value: Value, n: u8) -> Value {
    let mut value = value;
    value["node_id"] = json!(node.id);
    value["at"] = json!(now());
    let payload = value.to_string();
    serde_json::to_value(SignedPayload {
        signature: crypto::sign(&key(n), &payload),
        payload,
    })
    .unwrap()
}

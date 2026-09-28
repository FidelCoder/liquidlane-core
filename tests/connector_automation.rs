mod support;
use axum::{Json, Router, routing::post};
use liquidlane_core::{
    connector::{
        Connector, Registration,
        config::{Config, write_private},
    },
    marketplace::{
        model::{Order, TESTNET_GENESIS},
        router,
    },
};
use serde_json::{Value, json};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use support::*;

async fn serve(app: Router) -> (String, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let handle = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (url, handle)
}

#[tokio::test]
async fn automatic_connector_opens_without_local_approval_and_never_replays_an_uncertain_attempt() {
    // Real signatures and coordinator; controlled RPC responses exercise delayed native funding.
    let state = market();
    seed(&state, 5000);
    let app = router(state.clone()).unwrap();
    let first = create(&app).await;
    let (status, second) = request(
        &app,
        "/market/orders",
        Some("second-account"),
        Some(order_body("second", "new-merchant-no-allowlist")),
    )
    .await;
    assert_eq!(status, 200);
    let second: Order = serde_json::from_value(second).unwrap();
    let (core_url, core) = serve(app.clone()).await;
    let openings = Arc::new(AtomicUsize::new(0));
    let calls = openings.clone();
    let (rpc_url, rpc) = serve(Router::new().route("/", post(move |Json(request): Json<Value>| {
        let calls = calls.clone();
        async move {
            let result = match request["method"].as_str().unwrap() {
                "node_info" => json!({"pubkey":node("provider",1,"provider",5000).pubkey,"version":"0.9.0","chain_hash":TESTNET_GENESIS,"default_funding_lock_script": {"code_hash": "0x".to_owned()+&"01".repeat(32),"hash_type":"type","args":"0x"}}),
                "get_cells" => json!({"objects":[{"output":{"capacity":format!("0x{:x}",5000_u64 * 100_000_000),"type":null},"output_data":"0x"}]}),
                "list_channels" => json!({"channels":[]}),
                "connect_peer" => json!({}),
                "list_peers" => json!({"peers":[{"pubkey":node("merchant",2,"merchant",1000).pubkey},{"pubkey":node("second",3,"merchant",1000).pubkey}]}),
                "open_channel" => {
                    assert_eq!(request["params"][0]["funding_amount"], format!("0x{:x}",600_u64*100_000_000));
                    assert_eq!(request["params"][0]["one_way"], false);
                    calls.fetch_add(1, Ordering::SeqCst);
                    json!({"temporary_channel_id":"temporary-channel"})
                }
                method => panic!("unexpected RPC: {method}"),
            };
            Json(json!({"jsonrpc":"2.0","id":request["id"],"result":result}))
        }
    }))).await;
    let directory =
        std::env::temp_dir().join(format!("liquidlane-automation-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(directory.join("key"), key(1).secret_bytes()).unwrap();
    write_private(
        &directory.join("registration.json"),
        &Registration {
            token: "provider-node".into(),
            node: node("provider", 1, "provider", 5000),
            expires_at: i64::MAX,
        },
    )
    .unwrap();
    let config = Config {
        core_url,
        fiber_rpc: rpc_url.clone(),
        ckb_rpc: rpc_url,
        node_key_file: directory.join("key"),
        state_dir: directory.clone(),
        node_address: node("provider", 1, "provider", 5000).address,
        owner_address: address(1),
        max_order_ckb: 900,
        max_total_ckb: 1000,
        min_fee_ckb: 61,
        allowed_merchants: vec![],
        accept_public_orders: true,
        auto_approve: true,
        fiber_version: "0.9.0".into(),
    };
    let connector = Connector::load(config.clone()).unwrap();
    connector.tick().await.unwrap();
    for (order, token, n) in [
        (&first, "merchant-account", 2),
        (&second, "second-account", 3),
    ] {
        assert_eq!(
            request(
                &app,
                &format!("/market/orders/{}/accept", order.id),
                Some(token),
                Some(json!({"proof":proof(n,&order.approval_message())}))
            )
            .await
            .0,
            200
        );
    }
    connector.tick().await.unwrap();
    assert_eq!(openings.load(Ordering::SeqCst), 1);
    let journals = [
        connector.journal(&first.id).unwrap(),
        connector.journal(&second.id).unwrap(),
    ];
    assert_eq!(journals.iter().filter(|j| j.attempted).count(), 1);
    assert!(journals.iter().all(|j| !j.approved));
    drop(connector);
    let connector = Connector::load(config).unwrap();
    connector.tick().await.unwrap();
    assert_eq!(openings.load(Ordering::SeqCst), 1);
    let lock = std::fs::OpenOptions::new()
        .write(true)
        .open(directory.join("worker.lock"))
        .unwrap();
    lock.lock().unwrap();
    assert!(
        connector
            .tick()
            .await
            .unwrap_err()
            .to_string()
            .contains("already running")
    );
    drop(lock);
    drop(connector);
    core.abort();
    rpc.abort();
    std::fs::remove_dir_all(directory).unwrap();
}

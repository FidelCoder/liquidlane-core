#[path = "support/recovery.rs"]
mod recovery;
mod support;
use liquidlane_core::{
    connector::{
        Connector, Registration,
        config::{Config, write_private},
    },
    marketplace::{db, model::now},
};
use serde_json::{Value, json};

#[tokio::test]
async fn expired_confirmation_is_not_renewed_when_rpc_fails_or_after_restart() {
    let rpc = recovery::serve(recovery::records(), false).await;
    let directory = std::env::temp_dir().join(format!(
        "liquidlane-recovery-cache-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir(&directory).unwrap();
    std::fs::write(directory.join("key"), support::key(1).secret_bytes()).unwrap();
    write_private(
        &directory.join("registration.json"),
        &Registration {
            token: "local-test".into(),
            node: support::node("provider", 1, "provider", 0),
            expires_at: now() + 3600,
        },
    )
    .unwrap();
    let mut config = Config {
        core_url: "http://127.0.0.1:1".into(),
        fiber_rpc: "http://127.0.0.1:1".into(),
        ckb_rpc: rpc.url.clone(),
        node_key_file: directory.join("key"),
        state_dir: directory.clone(),
        node_address: support::node("provider", 1, "provider", 0).address,
        owner_address: support::address(1),
        max_order_ckb: 1000,
        max_total_ckb: 2000,
        min_fee_ckb: 61,
        allowed_merchants: vec![],
        accept_public_orders: true,
        auto_approve: true,
        fiber_version: "0.9.0".into(),
    };
    let connector = Connector::load(config.clone()).unwrap();
    let channels = [
        json!({"channel_id":"native-id","channel_outpoint":recovery::FIRST,
        "state":"Closed","local_balance":"0x25a01c500"}),
    ];
    assert_eq!(
        connector
            .provider_policy(&channels)
            .await
            .unwrap()
            .committed_ckb,
        0
    );
    let mut cache: Value = connector.db.get("settlement", recovery::FIRST).unwrap();
    assert_eq!(cache["settlement"]["confirmed"], true);
    cache["checked_at"] = json!(now() - 61);
    connector
        .db
        .transaction(|tx| db::put(tx, "settlement", recovery::FIRST, "local", &cache))
        .unwrap();
    drop(connector);
    drop(rpc);
    config.ckb_rpc = "http://127.0.0.1:1".into();
    let connector = Connector::load(config).unwrap();
    assert_eq!(
        connector
            .provider_policy(&channels)
            .await
            .unwrap()
            .committed_ckb,
        200
    );
    let cache: Value = connector.db.get("settlement", recovery::FIRST).unwrap();
    assert!(cache["settlement"].is_null());
    assert!(
        connector
            .settlement(recovery::FIRST)
            .await
            .unwrap()
            .is_none()
    );
    drop(connector);
    std::fs::remove_dir_all(directory).unwrap();
}

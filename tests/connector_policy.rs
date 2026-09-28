mod support;
use liquidlane_core::{
    connector::{
        Connector, Registration,
        config::{Config, write_private},
    },
    marketplace::{crypto, model::Order, router},
};
use support::*;

#[tokio::test]
async fn malicious_coordinator_cannot_alter_approved_funding_peer_or_recipient() {
    let state = market();
    seed(&state, 3000);
    let mut order = create(&router(state).unwrap()).await;
    order.provider_signature = Some(crypto::sign(
        &key(1),
        &crypto::quote_message(&order.quote).unwrap(),
    ));
    order.merchant_proof = Some(serde_json::to_value(proof(2, &order.approval_message())).unwrap());
    let path = std::env::temp_dir().join(format!("liquidlane-policy-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&path).unwrap();
    std::fs::write(path.join("key"), key(1).secret_bytes()).unwrap();
    let registration = Registration {
        token: "local-test-credential".into(),
        node: node("provider", 1, "provider", 3000),
        expires_at: i64::MAX,
    };
    write_private(&path.join("registration.json"), &registration).unwrap();
    let config = Config {
        core_url: "http://127.0.0.1:1".into(),
        fiber_rpc: "http://127.0.0.1:1".into(),
        ckb_rpc: "http://127.0.0.1:1".into(),
        node_key_file: path.join("key"),
        state_dir: path.clone(),
        node_address: registration.node.address.clone(),
        owner_address: address(1),
        max_order_ckb: 2000,
        max_total_ckb: 5000,
        min_fee_ckb: 61,
        allowed_merchants: vec![address(2)],
        accept_public_orders: false,
        auto_approve: false,
        fiber_version: "0.9.0".into(),
    };
    let mut legacy = serde_json::to_value(&config).unwrap();
    legacy
        .as_object_mut()
        .unwrap()
        .remove("accept_public_orders");
    assert!(
        !serde_json::from_value::<Config>(legacy)
            .unwrap()
            .accept_public_orders
    );
    let mut connector = Connector::load(config).unwrap();
    // A manually launched connector and a background service cannot run together.
    let guard = connector.run_lock().unwrap();
    assert!(connector.run_lock().is_err());
    drop(guard);
    drop(connector.run_lock().unwrap());
    connector.validate_order(&order, true).await.unwrap();
    connector.config.allowed_merchants.clear();
    assert!(connector.validate_order(&order, true).await.is_err());
    connector.config.accept_public_orders = true;
    connector.config.auto_approve = true;
    // A properly signed order from a previously unknown merchant is eligible without an allowlist.
    connector.validate_order(&order, true).await.unwrap();
    for mutate in [
        |o: &mut Order| o.quote.fee_recipient = address(3),
        |o: &mut Order| o.quote.merchant_pubkey = crypto::pubkey(&key(3)),
        |o: &mut Order| {
            o.quote.capacity_ckb = 800;
            o.quote.funding_ckb = 900;
        },
        |o: &mut Order| o.quote.expires_at = 0,
        |o: &mut Order| o.quote.network = "mainnet".into(),
        |o: &mut Order| o.quote.provider_reserve_ckb = 0,
    ] {
        let mut changed = order.clone();
        mutate(&mut changed);
        changed.quote_hash =
            crypto::digest(crypto::quote_message(&changed.quote).unwrap().as_bytes());
        assert!(connector.validate_order(&changed, true).await.is_err());
    }
    let mut journal = connector.journal(&order.id).unwrap();
    journal.attempted = true;
    journal.quote_hash = order.quote_hash.clone();
    connector.save(&journal).unwrap();
    // This returns before any RPC: an uncertain previous submission never gets opened twice.
    connector.open_order(&order).await.unwrap();
    assert!(connector.journal(&order.id).unwrap().attempted);
    drop(connector);
    std::fs::remove_dir_all(path).unwrap();
}

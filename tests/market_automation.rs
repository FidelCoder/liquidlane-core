mod support;
use liquidlane_core::marketplace::{
    db,
    model::{Node, ProviderPolicy},
    router,
};
use serde_json::json;
use support::*;

fn provider_policy(state: &liquidlane_core::marketplace::Market, committed: u64, minimum_fee: u64) {
    let mut provider: Node = state.db.get("node", "provider").unwrap();
    provider.provider_policy = Some(ProviderPolicy {
        accept_public_orders: true,
        auto_approve: true,
        max_order_ckb: 900,
        max_total_ckb: 1500,
        min_fee_ckb: minimum_fee,
        committed_ckb: committed,
    });
    state
        .db
        .transaction(|tx| db::put(tx, "node", &provider.id, &provider.owner, &provider))
        .unwrap();
}

#[tokio::test]
async fn public_orders_need_no_merchant_list_and_reserve_the_remaining_budget() {
    let state = market();
    seed(&state, 10_000);
    provider_policy(&state, 800, 61);
    let app = router(state).unwrap();
    let (_, listings) = request(&app, "/market/offers", None, None).await;
    assert_eq!(listings["offers"][0]["automatic"], true);
    assert_eq!(listings["offers"][0]["available_capacity_ckb"], 600);
    let (first, second) = tokio::join!(
        request(
            &app,
            "/market/orders",
            Some("merchant-account"),
            Some(order_body("merchant", "public-merchant-request"))
        ),
        request(
            &app,
            "/market/orders",
            Some("second-account"),
            Some(order_body("second", "another-public-request"))
        )
    );
    assert_eq!([first.0, second.0].iter().filter(|s| **s == 200).count(), 1);
    assert_eq!(
        if first.0 == 200 {
            first.1["automatic"].clone()
        } else {
            second.1["automatic"].clone()
        },
        true
    );
    let (_, listings) = request(&app, "/market/offers", None, None).await;
    assert_eq!(listings["offers"][0]["available_capacity_ckb"], 0);
}

#[tokio::test]
async fn listing_and_orders_respect_per_order_limits_and_fee_changes() {
    let state = market();
    seed(&state, 10_000);
    provider_policy(&state, 0, 61);
    let app = router(state.clone()).unwrap();
    let (_, listings) = request(&app, "/market/offers", None, None).await;
    assert_eq!(listings["offers"][0]["available_capacity_ckb"], 800);
    let mut too_large = order_body("merchant", "too-large-public-request");
    too_large["capacity_ckb"] = json!(801);
    assert_eq!(
        request(
            &app,
            "/market/orders",
            Some("merchant-account"),
            Some(too_large)
        )
        .await
        .0,
        400
    );
    provider_policy(&state, 0, 63);
    let (_, listings) = request(&app, "/market/offers", None, None).await;
    assert_eq!(listings["offers"][0]["available_capacity_ckb"], 0);
    assert_eq!(
        request(
            &app,
            "/market/orders",
            Some("merchant-account"),
            Some(order_body("merchant", "old-fee-public-request"))
        )
        .await
        .0,
        400
    );
    let offer = json!({"provider_node":"provider","min_capacity_ckb":100,"max_capacity_ckb":800,"opening_fee_ckb":61,"public_channel":true});
    assert_eq!(
        request(
            &app,
            "/market/offers",
            Some("provider-account"),
            Some(offer.clone())
        )
        .await
        .0,
        400
    );
    let mut offer = offer;
    offer["opening_fee_ckb"] = json!(63);
    assert_eq!(
        request(
            &app,
            "/market/offers",
            Some("provider-account"),
            Some(offer.clone())
        )
        .await
        .0,
        200
    );
    offer["max_capacity_ckb"] = json!(801);
    assert_eq!(
        request(
            &app,
            "/market/offers",
            Some("provider-account"),
            Some(offer)
        )
        .await
        .0,
        400
    );
}

#[tokio::test]
async fn signed_heartbeat_reports_automation_without_broadening_old_connectors() {
    let state = market();
    seed(&state, 3000);
    let app = router(state.clone()).unwrap();
    let node: Node = state.db.get("node", "provider").unwrap();
    assert!(!node.automatic());
    assert!(!node.background);
    let policy = json!({"accept_public_orders":true,"auto_approve":true,"max_order_ckb":900,"max_total_ckb":1500,"min_fee_ckb":61,"committed_ckb":800});
    let body = json!({"available_ckb":3000,"reserve_ckb":99,"version":"0.9.0","chain_hash":liquidlane_core::marketplace::model::TESTNET_GENESIS,"provider_policy":policy,"background":true});
    let mut bad = signed(&node, body.clone(), 1);
    bad["signature"] = json!("00".repeat(64));
    assert_eq!(
        request(
            &app,
            "/connector/heartbeat",
            Some("provider-node"),
            Some(bad)
        )
        .await
        .0,
        400
    );
    assert!(!state.db.get::<Node>("node", "provider").unwrap().background);
    assert_eq!(
        request(
            &app,
            "/connector/heartbeat",
            Some("provider-node"),
            Some(signed(&node, body.clone(), 1))
        )
        .await
        .0,
        200
    );
    let node: Node = state.db.get("node", "provider").unwrap();
    assert!(node.automatic());
    assert!(node.background);
    let mut old = body;
    old.as_object_mut().unwrap().remove("provider_policy");
    old.as_object_mut().unwrap().remove("background");
    assert_eq!(
        request(
            &app,
            "/connector/heartbeat",
            Some("provider-node"),
            Some(signed(&node, old, 1))
        )
        .await
        .0,
        200
    );
    assert!(
        !state
            .db
            .get::<Node>("node", "provider")
            .unwrap()
            .automatic()
    );
    assert!(!state.db.get::<Node>("node", "provider").unwrap().background);
}

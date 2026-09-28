mod support;
use liquidlane_core::marketplace::{
    crypto, db,
    model::{Node, TESTNET_GENESIS, now},
    router,
};
use serde_json::{Value, json};
use support::*;

fn report() -> Value {
    json!({"available_ckb":663,"reserve_ckb":99,"version":"0.9.0","chain_hash":TESTNET_GENESIS,"funding_address":address(5)})
}

#[tokio::test]
async fn funding_address_preserves_the_node_signature_and_rejects_changes() {
    let state = market();
    seed(&state, 0);
    let app = router(state.clone()).unwrap();
    let node: Node = state.db.get("node", "provider").unwrap();
    let envelope = signed(&node, report(), 1);
    let (status, response) = request(
        &app,
        "/connector/heartbeat",
        Some("provider-node"),
        Some(envelope.clone()),
    )
    .await;
    assert_eq!(status, 200, "{response}");
    let updated: Node = state.db.get("node", "provider").unwrap();
    let funding = updated.funding.unwrap();
    assert_eq!(funding.address, address(5));
    assert_eq!(funding.payload, envelope["payload"].as_str().unwrap());
    assert_eq!(funding.signature, envelope["signature"].as_str().unwrap());
    crypto::verify(&node.pubkey, &funding.payload, &funding.signature).unwrap();
    assert_eq!(updated.available_ckb, 663);

    let mut altered = envelope.clone();
    let mut payload: Value = serde_json::from_str(altered["payload"].as_str().unwrap()).unwrap();
    payload["funding_address"] = json!(address(6));
    altered["payload"] = json!(payload.to_string());
    assert_eq!(
        request(
            &app,
            "/connector/heartbeat",
            Some("provider-node"),
            Some(altered)
        )
        .await
        .0,
        400
    );
    assert_eq!(
        request(
            &app,
            "/connector/heartbeat",
            Some("merchant-node"),
            Some(envelope)
        )
        .await
        .0,
        400
    );
    assert_eq!(
        state
            .db
            .get::<Node>("node", "provider")
            .unwrap()
            .funding
            .unwrap()
            .address,
        address(5)
    );

    let mut invalid = report();
    invalid["funding_address"] = json!("ckt1invalid");
    assert_eq!(
        request(
            &app,
            "/connector/heartbeat",
            Some("provider-node"),
            Some(signed(&node, invalid, 1))
        )
        .await
        .0,
        400
    );
    let mut old_connector = report();
    old_connector
        .as_object_mut()
        .unwrap()
        .remove("funding_address");
    assert_eq!(
        request(
            &app,
            "/connector/heartbeat",
            Some("provider-node"),
            Some(signed(&node, old_connector, 1))
        )
        .await
        .0,
        200
    );
    assert!(
        state
            .db
            .get::<Node>("node", "provider")
            .unwrap()
            .funding
            .is_none()
    );
}

#[test]
fn funding_address_comes_from_the_funding_script_not_the_fiber_identity() {
    let expected = address(5);
    let script: ckb_jsonrpc_types::Script = crypto::address_script(&expected).unwrap().into();
    let actual = crypto::funding_address(&serde_json::to_value(script).unwrap()).unwrap();
    assert_eq!(actual, expected);
    assert_ne!(actual, address(1));
    assert!(crypto::funding_address(&json!({})).is_err());
}

#[tokio::test]
async fn publishing_requires_connected_capital_including_reserve_and_buffer() {
    let state = market();
    seed(&state, 0);
    let app = router(state.clone()).unwrap();
    let offer = json!({"provider_node":"provider","min_capacity_ckb":500,"max_capacity_ckb":1000,"opening_fee_ckb":61,"public_channel":true});
    for (capital, seen, expected) in [
        (0, now(), 400),
        (662, now(), 400),
        (663, now() - 91, 400),
        (663, now(), 200),
    ] {
        let mut node: Node = state.db.get("node", "provider").unwrap();
        node.available_ckb = capital;
        node.last_seen = seen;
        state
            .db
            .transaction(|tx| db::put(tx, "node", &node.id, &node.owner, &node))
            .unwrap();
        let (status, response) = request(
            &app,
            "/market/offers",
            Some("provider-account"),
            Some(offer.clone()),
        )
        .await;
        assert_eq!(
            status, expected,
            "capital={capital}, last_seen={seen}: {response}"
        );
        if expected == 400 {
            let prior: Value = state.db.get("offer", "offer").unwrap();
            assert_eq!(
                prior["enabled"], true,
                "failed publication must preserve the previous offer"
            );
        }
    }
}

#[tokio::test]
async fn pending_orders_consume_capital_before_another_offer_is_published() {
    let state = market();
    seed(&state, 1000);
    let app = router(state).unwrap();
    create(&app).await;
    let offer = json!({"provider_node":"provider","min_capacity_ckb":500,"max_capacity_ckb":1000,"opening_fee_ckb":61,"public_channel":true});
    let (status, response) = request(
        &app,
        "/market/offers",
        Some("provider-account"),
        Some(offer),
    )
    .await;
    assert_eq!(status, 400, "{response}");
    assert!(response["error"].as_str().unwrap().contains("add capital"));
}

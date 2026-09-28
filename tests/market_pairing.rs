mod support;
use liquidlane_core::marketplace::{
    crypto, db,
    model::{TESTNET_GENESIS, now},
    nodes::registration_message,
    router,
};
use serde_json::{Value, json};
use support::*;

#[tokio::test]
async fn pairing_progress_is_owner_only_and_confirms_the_exact_registered_node() {
    let state = market();
    seed(&state, 2000);
    let app = router(state.clone()).unwrap();
    let (status, pair) = request(
        &app,
        "/market/nodes/pair",
        Some("merchant-account"),
        Some(json!({"role":"merchant","label":"Receiving node"})),
    )
    .await;
    assert_eq!(status, 200);
    let path = format!(
        "/market/nodes/pair/{}",
        pair["pairing_id"].as_str().unwrap()
    );
    assert_eq!(request(&app, &path, None, None).await.0, 401);
    assert_eq!(
        request(&app, &path, Some("second-account"), None).await.0,
        400
    );
    let (_, waiting) = request(&app, &path, Some("merchant-account"), None).await;
    assert_eq!(waiting["status"], "waiting");
    assert!(
        waiting["node"].is_null(),
        "an existing account node must not complete this pairing"
    );
    let code = pair["pairing_code"].as_str().unwrap();
    let pubkey = crypto::pubkey(&key(4));
    let address = "/ip4/127.0.0.1/tcp/8228/p2p/receiving-node";
    let body = json!({"pairing_code":code,"pubkey":pubkey,"address":address,"version":"0.9.0","chain_hash":TESTNET_GENESIS,"signature":crypto::sign(&key(4),&registration_message(code,&pubkey,address))});
    let (status, registered) = request(&app, "/connector/register", None, Some(body.clone())).await;
    assert_eq!(status, 200, "{registered}");
    let (_, completed) = request(&app, &path, Some("merchant-account"), None).await;
    assert_eq!(completed["status"], "paired");
    assert_eq!(completed["node"]["id"], registered["node"]["id"]);
    assert_eq!(
        completed["node"]["last_seen"], 0,
        "pairing alone must not claim the node is online"
    );
    assert!(completed.get("token").is_none());
    assert!(completed.get("pairing_code").is_none());
    assert_eq!(
        request(&app, "/connector/register", None, Some(body))
            .await
            .0,
        404
    );
    assert_eq!(
        request(&app, &path, Some("second-account"), None).await.0,
        400
    );
    let stored: Value = state
        .db
        .get("pairing_status", pair["pairing_id"].as_str().unwrap())
        .unwrap();
    assert_eq!(stored["node_id"], completed["node"]["id"]);
}

#[tokio::test]
async fn expired_pairing_reports_expiry_without_registering_a_node() {
    let state = market();
    seed(&state, 2000);
    let app = router(state.clone()).unwrap();
    let (_, pair) = request(
        &app,
        "/market/nodes/pair",
        Some("merchant-account"),
        Some(json!({"role":"merchant","label":"Expired setup"})),
    )
    .await;
    let id = pair["pairing_id"].as_str().unwrap();
    state
        .db
        .transaction(|tx| {
            for kind in ["pairing", "pairing_status"] {
                let mut stored: Value = db::get(tx, kind, id)?;
                stored["expires_at"] = json!(now() - 1);
                db::put(tx, kind, id, &address(2), &stored)?;
            }
            Ok(())
        })
        .unwrap();
    let (_, result) = request(
        &app,
        &format!("/market/nodes/pair/{id}"),
        Some("merchant-account"),
        None,
    )
    .await;
    assert_eq!(result["status"], "expired");
    assert!(result["node"].is_null());
    let code = pair["pairing_code"].as_str().unwrap();
    let pubkey = crypto::pubkey(&key(4));
    let peer = "/ip4/127.0.0.1/tcp/8228/p2p/expired-node";
    let body = json!({"pairing_code":code,"pubkey":pubkey,"address":peer,"version":"0.9.0","chain_hash":TESTNET_GENESIS,"signature":crypto::sign(&key(4),&registration_message(code,&pubkey,peer))});
    assert_eq!(
        request(&app, "/connector/register", None, Some(body))
            .await
            .0,
        400
    );
}

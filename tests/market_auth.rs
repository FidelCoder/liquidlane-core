mod support;
use liquidlane_core::marketplace::{
    crypto,
    model::{TESTNET_GENESIS, now},
    nodes::registration_message,
    router,
};
use serde_json::json;
use support::*;

#[tokio::test]
async fn real_signature_login_rejects_address_substitution_and_replay() {
    let state = market();
    let app = router(state.clone()).unwrap();
    let (status, challenge) = request(
        &app,
        "/auth/challenge",
        None,
        Some(json!({"address":address(2)})),
    )
    .await;
    assert_eq!(status, 200);
    let message = challenge["message"].as_str().unwrap();
    assert!(message.contains("Origin: http://localhost:3000\nNetwork: testnet"));
    let mut forged = proof(3, message);
    forged.address = address(2);
    let body = json!({"challenge_id":challenge["challenge_id"],"proof":forged});
    assert_eq!(request(&app, "/auth/verify", None, Some(body)).await.0, 400);
    let body = json!({"challenge_id":challenge["challenge_id"],"proof":proof(2,message)});
    let (status, session) = request(&app, "/auth/verify", None, Some(body.clone())).await;
    assert_eq!(status, 200, "{session}");
    assert_eq!(request(&app, "/auth/verify", None, Some(body)).await.0, 401);
    let token = session["token"].as_str().unwrap();
    assert_eq!(
        request(&app, "/market/dashboard", Some(token), None)
            .await
            .0,
        200
    );
    assert_eq!(
        request(&app, "/connector/orders", Some(token), None)
            .await
            .0,
        401
    );
    assert_eq!(
        request(&app, "/auth/logout", Some(token), Some(json!({})))
            .await
            .0,
        200
    );
    assert_eq!(
        request(&app, "/market/dashboard", Some(token), None)
            .await
            .0,
        401
    );
}

#[tokio::test]
async fn expired_challenges_and_legacy_login_are_rejected() {
    let state = market();
    let app = router(state.clone()).unwrap();
    let (_, challenge) = request(
        &app,
        "/auth/challenge",
        None,
        Some(json!({"address":address(2)})),
    )
    .await;
    state
        .db
        .transaction(|tx| {
            tx.execute("UPDATE challenges SET expires=?1", [now() - 1])?;
            Ok(())
        })
        .unwrap();
    let body = json!({"challenge_id":challenge["challenge_id"],"proof":proof(2,challenge["message"].as_str().unwrap())});
    assert_eq!(request(&app, "/auth/verify", None, Some(body)).await.0, 401);
    assert_eq!(
        request(
            &app,
            "/auth/connect",
            None,
            Some(json!({"ckb_address":address(2)}))
        )
        .await
        .0,
        404
    );
    assert_eq!(request(&app, "/vault", None, None).await.0, 404);
    assert_eq!(request(&app, "/market/dashboard", None, None).await.0, 401);
}

#[tokio::test]
async fn pairing_requires_node_control_and_cannot_transfer_ownership() {
    let state = market();
    seed(&state, 2000);
    let app = router(state).unwrap();
    let (_, pair) = request(
        &app,
        "/market/nodes/pair",
        Some("provider-account"),
        Some(json!({"role":"provider","label":"Provider"})),
    )
    .await;
    let pubkey = crypto::pubkey(&key(1));
    let address = "/ip4/127.0.0.1/tcp/8228/p2p/test-provider";
    let code = pair["pairing_code"].as_str().unwrap();
    let mut body = json!({"pairing_code":code,"pubkey":pubkey,"address":address,"version":"0.9.0","chain_hash":TESTNET_GENESIS,"signature":crypto::sign(&key(2),&registration_message(code,&pubkey,address))});
    assert_eq!(
        request(&app, "/connector/register", None, Some(body.clone()))
            .await
            .0,
        400
    );
    body["signature"] = json!(crypto::sign(
        &key(1),
        &registration_message(code, &pubkey, address)
    ));
    let (status, result) = request(&app, "/connector/register", None, Some(body.clone())).await;
    assert_eq!(status, 200, "{result}");
    assert_eq!(
        request(&app, "/connector/register", None, Some(body))
            .await
            .0,
        404
    );
    assert_eq!(
        request(&app, "/connector/orders", Some("provider-node"), None)
            .await
            .0,
        401
    );
    assert_eq!(
        request(
            &app,
            "/market/dashboard",
            Some(result["token"].as_str().unwrap()),
            None
        )
        .await
        .0,
        401
    );
    let (_, pair) = request(
        &app,
        "/market/nodes/pair",
        Some("merchant-account"),
        Some(json!({"role":"provider","label":"Imposter"})),
    )
    .await;
    let code = pair["pairing_code"].as_str().unwrap();
    let body = json!({"pairing_code":code,"pubkey":pubkey,"address":address,"version":"0.9.0","chain_hash":TESTNET_GENESIS,"signature":crypto::sign(&key(1),&registration_message(code,&pubkey,address))});
    assert_eq!(
        request(&app, "/connector/register", None, Some(body))
            .await
            .0,
        400
    );
}

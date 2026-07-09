use std::{path::PathBuf, sync::Arc};

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use serde_json::json;
use tower::ServiceExt;

use crate::{
    http::{AppState, router},
    store::AppStore,
};

pub(super) fn test_app() -> Router {
    test_app_with_script_build_dir(PathBuf::from("./ckb-scripts/build"))
}

pub(super) fn test_app_with_script_build_dir(ckb_script_build_dir: PathBuf) -> Router {
    router(AppState {
        environment: "test".to_string(),
        store: Arc::new(AppStore::memory()),
        fiber_rpc_configured: false,
        ckb_rpc_configured: false,
        ckb_script_build_dir,
    })
}

pub(super) async fn auth_token(app: Router, name: &str, role: &str, ckb_address: &str) -> String {
    let connect_response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/connect")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "ckb_address": ckb_address,
                        "wallet_type": "joyid_ckb",
                        "role": role,
                        "lock_script": {
                            "code_hash": "0x0000000000000000000000000000000000000000000000000000000000000000",
                            "hash_type": "type",
                            "args": "0x1234"
                        },
                        "display_name": name
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(connect_response.status(), StatusCode::CREATED);
    let body = connect_response
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes();
    serde_json::from_slice::<serde_json::Value>(&body).unwrap()["token"]
        .as_str()
        .unwrap()
        .to_string()
}

pub(super) fn signed_tx_fixture(tx_hash: &str) -> serde_json::Value {
    json!({
        "hash": tx_hash,
        "cellDeps": [],
        "headerDeps": [],
        "inputs": [{"previousOutput":{"txHash":"0x2222222222222222222222222222222222222222222222222222222222222222","index":"0x0"},"since":"0x0"}],
        "outputs": [{"capacity":"0x174876e800","lock":{"codeHash":"0x3333333333333333333333333333333333333333333333333333333333333333","hashType":"type","args":"0x1234"}}],
        "outputsData": ["0x"],
        "version": "0x0",
        "witnesses": ["0x55000000100000005500000055000000410000001111111122222222333333334444444455555555666666667777777788888888"]
    })
}

pub(super) async fn create_supply_intent(
    app: Router,
    token: &str,
    amount: u64,
) -> serde_json::Value {
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/vault/supply/intents")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::from(
                    json!({"asset":"CKB","amount":amount}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&body).unwrap()
}

pub(super) async fn settle_supply(
    app: Router,
    token: &str,
    intent: &serde_json::Value,
    amount: u64,
    tx_hash: &str,
) {
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/deposits")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::from(
                    json!({
                        "asset":"CKB",
                        "amount":amount,
                        "intent_id": intent["id"],
                        "tx_hash": tx_hash,
                        "signed_tx": signed_tx_fixture(tx_hash)
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
}

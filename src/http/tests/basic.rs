use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use serde_json::json;
use tower::ServiceExt;

use super::support::{auth_token, create_supply_intent, signed_tx_fixture, test_app};

#[tokio::test]
async fn exposes_active_vault_config() {
    let response = test_app()
        .oneshot(
            Request::builder()
                .uri("/vault")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let vault: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(vault["asset"], "CKB");
    assert_eq!(vault["network"], "testnet");
    assert!(vault["configured"].as_bool().unwrap());
    assert!(vault["address"].as_str().unwrap().starts_with("ckt"));
}

#[tokio::test]
async fn protected_dashboard_requires_auth() {
    let response = test_app()
        .oneshot(
            Request::builder()
                .uri("/dashboard")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn lp_deposit_requires_signed_transaction_proof() {
    let app = test_app();
    let lp_token = auth_token(
        app.clone(),
        "Atlas LP",
        "lp",
        "ckt1qyq000000000000000000000000000000000000lp",
    )
    .await;

    let deposit_response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/deposits")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, format!("Bearer {lp_token}"))
                .body(Body::from(json!({"asset":"CKB","amount":5000}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(deposit_response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn lp_deposit_rejects_signed_transaction_hash_mismatch() {
    let app = test_app();
    let lp_token = auth_token(
        app.clone(),
        "Atlas LP",
        "lp",
        "ckt1qyq000000000000000000000000000000000000lp",
    )
    .await;
    let intent = create_supply_intent(app.clone(), &lp_token, 5000).await;

    let request_hash = "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let signed_hash = "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/deposits")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, format!("Bearer {lp_token}"))
                .body(Body::from(
                    json!({
                        "asset": "CKB",
                        "amount": 5000,
                        "intent_id": intent["id"],
                        "tx_hash": request_hash,
                        "signed_tx": signed_tx_fixture(signed_hash)
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

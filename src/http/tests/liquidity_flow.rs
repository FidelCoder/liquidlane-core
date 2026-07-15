use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use serde_json::json;
use tower::ServiceExt;

use super::support::{auth_token, create_supply_intent, settle_supply, test_app};

#[tokio::test]
async fn lp_deposit_then_merchant_request_and_queue_fiber_channel() {
    let app = test_app();
    let lp_token = auth_token(
        app.clone(),
        "Atlas LP",
        "lp",
        "ckt1qyq000000000000000000000000000000000000lp",
    )
    .await;
    let merchant_token = auth_token(
        app.clone(),
        "Kairo Market",
        "merchant",
        "ckt1qyq0000000000000000000000000000000merchant",
    )
    .await;

    let intent = create_supply_intent(app.clone(), &lp_token, 5000).await;
    assert_eq!(intent["status"], "pending_signature");
    assert_eq!(
        intent["vault_address"],
        "ckt1qpkp7qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqq"
    );
    settle_supply(
        app.clone(),
        &lp_token,
        &intent,
        5000,
        "0x1111111111111111111111111111111111111111111111111111111111111111",
    )
    .await;

    let request_body = create_capacity_request(app.clone(), &merchant_token).await;
    assert_eq!(request_body["status"], "requested");
    let id = request_body["id"].as_str().unwrap();

    let dashboard = fetch_dashboard(app.clone(), &lp_token).await;
    assert_eq!(dashboard["vault"]["available_liquidity"], 2000);
    assert_eq!(dashboard["vault"]["reserved_liquidity"], 3000);
    assert_eq!(dashboard["reservations"][0]["status"], "reserved");

    deploy_request_without_fiber_rpc(app.clone(), &merchant_token, id).await;

    let dashboard = fetch_dashboard(app.clone(), &lp_token).await;
    assert_eq!(dashboard["vault"]["reserved_liquidity"], 3000);
    assert_eq!(dashboard["vault"]["deployed_liquidity"], 0);
    assert_eq!(dashboard["vault"]["fees_earned"], 30);
    assert_eq!(dashboard["positions"][0]["fees_earned"], 30);
    assert_eq!(dashboard["reservations"][0]["status"], "reserved");
}

async fn create_capacity_request(app: axum::Router, token: &str) -> serde_json::Value {
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/liquidity/requests")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::from(
                    json!({"asset":"CKB","amount":3000,"duration_days":30,"receiver_ckb_address":"ckt1qzda0cr08m85hc8jlnfp3zer7xulejywt49kt2rr0vthywaa50xwsqwh04ftzgcaymffpf245m0rjvd30x4s3rgt9e64s"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    response_json(response).await
}

async fn deploy_request_without_fiber_rpc(app: axum::Router, token: &str, id: &str) {
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/liquidity/requests/{id}/deploy"))
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = response_json(response).await;
    assert!(
        body["error"]
            .as_str()
            .unwrap()
            .contains("on-chain capacity request")
    );
}

async fn fetch_dashboard(app: axum::Router, token: &str) -> serde_json::Value {
    let response = app
        .oneshot(
            Request::builder()
                .uri("/dashboard")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    response_json(response).await
}

async fn response_json(response: axum::response::Response) -> serde_json::Value {
    let body = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&body).unwrap()
}

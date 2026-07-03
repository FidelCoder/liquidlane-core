use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use serde_json::json;
use tower::ServiceExt;

use super::support::{
    auth_token, create_supply_intent, settle_supply, signed_tx_fixture, test_app,
};

#[tokio::test]
async fn lp_can_create_and_settle_withdrawal_intent() {
    let app = test_app();
    let lp_token = auth_token(
        app.clone(),
        "Atlas LP",
        "lp",
        "ckt1qyq000000000000000000000000000000000000lp",
    )
    .await;

    let intent = create_supply_intent(app.clone(), &lp_token, 5000).await;
    settle_supply(
        app.clone(),
        &lp_token,
        &intent,
        5000,
        "0x1111111111111111111111111111111111111111111111111111111111111111",
    )
    .await;

    let dashboard = fetch_dashboard(app.clone(), &lp_token).await;
    let position_id = dashboard["positions"][0]["id"].as_str().unwrap();
    assert_eq!(dashboard["vault"]["available_liquidity"], 5000);

    let withdrawal = create_withdrawal(app.clone(), &lp_token, position_id).await;
    let withdrawal_id = withdrawal["id"].as_str().unwrap();
    assert_eq!(withdrawal["status"], "pending_signature");

    settle_withdrawal(app.clone(), &lp_token, withdrawal_id).await;

    let dashboard = fetch_dashboard(app, &lp_token).await;
    assert_eq!(dashboard["vault"]["total_deposits"], 3000);
    assert_eq!(dashboard["vault"]["available_liquidity"], 3000);
    assert_eq!(dashboard["positions"][0]["supplied_amount"], 3000);
    assert_eq!(dashboard["withdrawals"][0]["status"], "settled");
}

async fn create_withdrawal(app: axum::Router, token: &str, position_id: &str) -> serde_json::Value {
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/vault/withdrawals/intents")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::from(
                    json!({"position_id": position_id, "amount": 2000}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    response_json(response).await
}

async fn settle_withdrawal(app: axum::Router, token: &str, withdrawal_id: &str) {
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/vault/withdrawals/{withdrawal_id}/settle"))
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::from(
                    json!({
                        "tx_hash":"0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                        "signed_tx": signed_tx_fixture("0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
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

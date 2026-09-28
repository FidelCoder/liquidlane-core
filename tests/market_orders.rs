mod support;
use liquidlane_core::marketplace::{
    crypto, db,
    model::{Order, now},
    orders, router,
};
use serde_json::json;
use support::*;

#[tokio::test]
async fn concurrent_orders_cannot_oversell_and_retries_are_idempotent() {
    let state = market();
    seed(&state, 800);
    let app = router(state.clone()).unwrap();
    let body = order_body("merchant", "idempotent-request-one");
    let (a, b) = tokio::join!(
        request(
            &app,
            "/market/orders",
            Some("merchant-account"),
            Some(body.clone())
        ),
        request(
            &app,
            "/market/orders",
            Some("second-account"),
            Some(order_body("second", "idempotent-request-two"))
        )
    );
    assert_eq!([a.0, b.0].iter().filter(|s| **s == 200).count(), 1);
    assert_eq!(state.db.list::<Order>("order").unwrap().len(), 1);
    let (token, body, winner) = if a.0 == 200 {
        ("merchant-account", body, a.1)
    } else {
        (
            "second-account",
            order_body("second", "idempotent-request-two"),
            b.1,
        )
    };
    let again = request(&app, "/market/orders", Some(token), Some(body.clone())).await;
    assert_eq!(again.0, 200);
    assert_eq!(again.1["id"], winner["id"]);
    let mut changed = body;
    changed["capacity_ckb"] = json!(600);
    assert_eq!(
        request(&app, "/market/orders", Some(token), Some(changed))
            .await
            .0,
        400
    );
}

#[tokio::test]
async fn two_signatures_are_required_and_opening_cannot_be_replayed_or_cancelled() {
    let state = market();
    seed(&state, 2000);
    let app = router(state.clone()).unwrap();
    let order = create(&app).await;
    let base = format!("/connector/orders/{}", order.id);
    let merchant = format!("/market/orders/{}", order.id);
    let start = signed(
        &node("provider", 1, "provider", 2000),
        json!({"order_id":order.id,"action":"start"}),
        1,
    );
    assert_eq!(
        request(
            &app,
            &format!("{base}/start"),
            Some("provider-node"),
            Some(start.clone())
        )
        .await
        .0,
        400
    );
    let sig = crypto::sign(&key(1), &crypto::quote_message(&order.quote).unwrap());
    assert_eq!(
        request(
            &app,
            &format!("{base}/quote"),
            Some("provider-node"),
            Some(json!({"signature":sig}))
        )
        .await
        .0,
        200
    );
    assert_eq!(
        request(
            &app,
            &format!("{merchant}/accept"),
            Some("second-account"),
            Some(json!({"proof":proof(2,&order.approval_message())}))
        )
        .await
        .0,
        400
    );
    assert_eq!(
        request(
            &app,
            &format!("{merchant}/accept"),
            Some("merchant-account"),
            Some(json!({"proof":proof(2,"another quote")}))
        )
        .await
        .0,
        400
    );
    assert_eq!(
        request(
            &app,
            &format!("{merchant}/accept"),
            Some("merchant-account"),
            Some(json!({"proof":proof(2,&order.approval_message())}))
        )
        .await
        .0,
        200
    );
    assert_eq!(
        request(
            &app,
            &format!("{base}/start"),
            Some("provider-node"),
            Some(start.clone())
        )
        .await
        .0,
        200
    );
    assert_eq!(
        request(
            &app,
            &format!("{base}/start"),
            Some("provider-node"),
            Some(start)
        )
        .await
        .0,
        400
    );
    assert_eq!(
        request(
            &app,
            &format!("{merchant}/cancel"),
            Some("merchant-account"),
            Some(json!({}))
        )
        .await
        .0,
        400
    );
    let mut persisted: Order = state.db.get("order", &order.id).unwrap();
    persisted.quote.expires_at = now() - 1;
    state
        .db
        .transaction(|tx| db::put(tx, "order", &order.id, &order.owner, &persisted))
        .unwrap();
    orders::expire(&state).unwrap();
    assert_eq!(
        state.db.get::<Order>("order", &order.id).unwrap().status,
        "opening"
    );
}

#[tokio::test]
async fn cancellation_releases_capacity_but_unpaid_delivery_blocks_new_work() {
    let state = market();
    seed(&state, 800);
    let app = router(state.clone()).unwrap();
    let order = create(&app).await;
    assert_eq!(
        request(
            &app,
            &format!("/market/orders/{}/cancel", order.id),
            Some("merchant-account"),
            Some(json!({}))
        )
        .await
        .0,
        200
    );
    let body = order_body("merchant", "a-new-order-request");
    let (status, value) =
        request(&app, "/market/orders", Some("merchant-account"), Some(body)).await;
    assert_eq!(status, 200);
    let mut delivered: Order = serde_json::from_value(value).unwrap();
    delivered.status = "delivered".into();
    delivered.fee_status = "due".into();
    delivered.delivered_at = Some(now() - 86401);
    state
        .db
        .transaction(|tx| db::put(tx, "order", &delivered.id, &delivered.owner, &delivered))
        .unwrap();
    orders::expire(&state).unwrap();
    assert_eq!(
        state
            .db
            .get::<Order>("order", &delivered.id)
            .unwrap()
            .fee_status,
        "overdue"
    );
    let body = order_body("merchant", "third-order-request");
    assert_eq!(
        request(
            &app,
            "/market/orders",
            Some("merchant-account"),
            Some(body.clone())
        )
        .await
        .0,
        400
    );
    assert_eq!(
        request(
            &app,
            &format!("/market/orders/{}/waive-fee", delivered.id),
            Some("merchant-account"),
            Some(json!({}))
        )
        .await
        .0,
        400
    );
    assert_eq!(
        request(
            &app,
            &format!("/market/orders/{}/waive-fee", delivered.id),
            Some("provider-account"),
            Some(json!({}))
        )
        .await
        .0,
        200
    );
    assert_eq!(
        request(&app, "/market/orders", Some("merchant-account"), Some(body))
            .await
            .0,
        200
    );
}

#[path = "support/recovery.rs"]
mod recovery;
mod support;
use liquidlane_core::marketplace::{chain, settlement};
use liquidlane_core::marketplace::{db, model::Order, router};
use recovery::*;
use serde_json::json;

#[tokio::test]
async fn recorded_forced_closures_resolve_without_native_shutdown_hashes() {
    let rpc = serve(records(), false).await;
    let client = reqwest::Client::new();
    for point in [FIRST, SECOND] {
        let proof = settlement::inspect(&client, &rpc.url, point)
            .await
            .unwrap()
            .unwrap();
        assert!(proof.confirmed);
        assert!(proof.pending_outpoints.is_empty());
        assert_eq!(proof.transaction_hashes.len(), 3);
        let verified = settlement::verify(&client, &rpc.url, point, &proof)
            .await
            .unwrap();
        assert_eq!(verified.closing_tx_hash, proof.closing_tx_hash);
    }
    // A wallet arrival that was later spent is still a completed settlement.
    // The scanner never follows ordinary wallet outputs or adds their gross change.
    let data = records();
    for request in rpc
        .requests
        .lock()
        .unwrap()
        .iter()
        .filter(|r| r["method"] == "get_live_cell")
    {
        let point = &request["params"][0];
        let tx = &data[point["tx_hash"].as_str().unwrap()]["transaction"];
        let index = chain::number(&point["index"]).unwrap() as usize;
        assert_ne!(
            tx["outputs"][index]["lock"]["code_hash"],
            "0x9bd7e06f3ecf4be0f2fcd2188b23f1b9fcc88e5d4b65a8637b17723bbda3cce8"
        );
    }
}

#[tokio::test]
async fn missing_indexer_spends_never_become_a_confirmation() {
    let rpc = serve(records(), true).await;
    assert!(
        settlement::inspect(&reqwest::Client::new(), &rpc.url, FIRST)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn spent_or_unavailable_funding_cannot_be_verified_for_delivery() {
    let rpc = serve(records(), false).await;
    let mut state = support::market();
    std::sync::Arc::make_mut(&mut state.config).ckb_rpc = rpc.url.clone();
    assert!(
        chain::verify_live_funding(&state, FIRST)
            .await
            .unwrap_err()
            .to_string()
            .contains("delivery cannot be verified")
    );
}

#[tokio::test]
async fn partial_settlement_and_forged_confirmation_are_distinct() {
    let mut data = records();
    data.remove("0xb48f813b002960cbaea25f94786089fc539edfa4356ac6d365b3d0b717d586e6");
    let rpc = serve(data, false).await;
    let client = reqwest::Client::new();
    let mut proof = settlement::inspect(&client, &rpc.url, FIRST)
        .await
        .unwrap()
        .unwrap();
    assert!(!proof.confirmed);
    assert_eq!(proof.pending_outpoints.len(), 1);
    settlement::verify(&client, &rpc.url, FIRST, &proof)
        .await
        .unwrap();
    proof.confirmed = true;
    proof.pending_outpoints.clear();
    assert!(
        settlement::verify(&client, &rpc.url, FIRST, &proof)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn unrelated_duplicate_or_uncommitted_transactions_cannot_release_a_channel() {
    let rpc = serve(records(), false).await;
    let client = reqwest::Client::new();
    let proof = settlement::inspect(&client, &rpc.url, FIRST)
        .await
        .unwrap()
        .unwrap();
    assert!(
        settlement::verify(&client, &rpc.url, SECOND, &proof)
            .await
            .is_err()
    );
    let mut duplicate = proof.clone();
    duplicate
        .transaction_hashes
        .push(proof.closing_tx_hash.clone());
    assert!(
        settlement::verify(&client, &rpc.url, FIRST, &duplicate)
            .await
            .is_err()
    );
    let mut unrelated = proof.clone();
    unrelated
        .transaction_hashes
        .push("0x48ec70fdcc1732b798d576284f9da107ef159ae95285d209aadfddd07fd5ed4a".into());
    assert!(
        settlement::verify(&client, &rpc.url, FIRST, &unrelated)
            .await
            .is_err()
    );
    let mut data = records();
    data.get_mut(&proof.closing_tx_hash).unwrap()["tx_status"]["status"] = json!("pending");
    let unavailable = serve(data, false).await;
    assert!(
        settlement::verify(&client, &unavailable.url, FIRST, &proof)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn unknown_or_typed_outputs_remain_unresolved() {
    for (field, value) in [
        (
            "lock",
            json!({"code_hash":"0xunknown","hash_type":"type","args":"0x"}),
        ),
        ("type", json!({"code_hash":"0xudt"})),
    ] {
        let mut data = records();
        data.get_mut("0xb48f813b002960cbaea25f94786089fc539edfa4356ac6d365b3d0b717d586e6")
            .unwrap()["transaction"]["outputs"][0][field] = value;
        let rpc = serve(data, false).await;
        let proof = settlement::inspect(&reqwest::Client::new(), &rpc.url, FIRST)
            .await
            .unwrap()
            .unwrap();
        assert!(!proof.confirmed);
        assert_eq!(proof.pending_outpoints.len(), 1);
    }
}

#[tokio::test]
async fn coordinator_rechecks_proofs_and_releases_only_undelivered_orders_without_a_fee() {
    let rpc = serve(records(), false).await;
    let client = reqwest::Client::new();
    let proof = settlement::inspect(&client, &rpc.url, FIRST)
        .await
        .unwrap()
        .unwrap();
    for delivered in [false, true] {
        let mut state = support::market();
        std::sync::Arc::make_mut(&mut state.config).ckb_rpc = rpc.url.clone();
        support::seed(&state, 2000);
        let app = router(state.clone()).unwrap();
        let mut order = support::create(&app).await;
        order.quote.funding_ckb = 200;
        order.status = if delivered { "delivered" } else { "opening" }.into();
        order.fee_status = if delivered { "paid" } else { "not_due" }.into();
        state
            .db
            .transaction(|tx| db::put(tx, "order", &order.id, &order.owner, &order))
            .unwrap();
        let path = format!("/connector/orders/{}/evidence", order.id);
        let node = support::node("provider", 1, "provider", 2000);
        let report = json!({"order_id":order.id,"evidence":{
            "channel_id":format!("0x{}", "aa".repeat(32)),
            "peer_pubkey":order.quote.merchant_pubkey,
            "funding_outpoint":FIRST,"state":"ShuttingDown","state_flags":"WAITING_COMMITMENT_CONFIRMATION",
            "local_balance":"0x0","remote_balance":"0x0","inbound_liquidity":"0x0",
            "observed_at":liquidlane_core::marketplace::model::now(),"settlement_tx_hash":null,
            "settlement":proof
        }});
        let mut forged = report.clone();
        forged["evidence"]["settlement"]["transaction_hashes"] = json!([proof.closing_tx_hash]);
        let (status, _) = support::request(
            &app,
            &path,
            Some("provider-node"),
            Some(support::signed(&node, forged, 1)),
        )
        .await;
        assert_ne!(status, 200);
        let (status, result) = support::request(
            &app,
            &path,
            Some("provider-node"),
            Some(support::signed(&node, report, 1)),
        )
        .await;
        assert_eq!(status, 200, "{result}");
        let updated: Order = serde_json::from_value(result).unwrap();
        assert_eq!(
            updated.status,
            if delivered { "delivered" } else { "failed" }
        );
        assert_eq!(
            updated.fee_status,
            if delivered { "paid" } else { "not_due" }
        );
        assert!(!updated.reserves_funds());
        assert!(
            updated
                .provider_evidence
                .unwrap()
                .settlement
                .unwrap()
                .confirmed
        );
    }
}

#[tokio::test]
async fn another_peers_ready_report_cannot_clear_pending_closure() {
    let rpc = serve(records(), false).await;
    let mut state = support::market();
    std::sync::Arc::make_mut(&mut state.config).ckb_rpc = rpc.url.clone();
    support::seed(&state, 2000);
    let app = router(state.clone()).unwrap();
    let mut order = support::create(&app).await;
    order.quote.funding_ckb = 200;
    order.status = "opening".into();
    state
        .db
        .transaction(|tx| db::put(tx, "order", &order.id, &order.owner, &order))
        .unwrap();
    for (role, n, native, peer) in [
        ("provider", 1, "Closed", &order.quote.merchant_pubkey),
        ("merchant", 2, "ChannelReady", &order.quote.provider_pubkey),
    ] {
        let report = json!({"order_id":order.id,"evidence":{
            "channel_id":format!("0x{}", "aa".repeat(32)),"peer_pubkey":peer,
            "funding_outpoint":FIRST,"state":native,"local_balance":"0x0","remote_balance":"0x0",
            "inbound_liquidity":"0x0","observed_at":liquidlane_core::marketplace::model::now(),"settlement_tx_hash":null
        }});
        let (status, result) = support::request(
            &app,
            &format!("/connector/orders/{}/evidence", order.id),
            Some(&format!("{role}-node")),
            Some(support::signed(
                &support::node(role, n, role, 2000),
                report,
                n,
            )),
        )
        .await;
        assert_eq!(status, 200, "{result}");
        let updated: Order = serde_json::from_value(result).unwrap();
        assert_eq!(updated.status, "reconciling");
        assert!(updated.reserves_funds());
        assert_eq!(updated.fee_status, "not_due");
    }
}

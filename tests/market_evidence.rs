mod support;
use liquidlane_core::marketplace::{
    chain,
    db::{self, Db},
    evidence,
    model::{ChannelEvidence, SHANNONS, now},
    router,
};
use serde_json::json;
use support::*;

#[tokio::test]
async fn delivery_needs_both_fresh_peers_capacity_and_actual_paid_probe() {
    let state = market();
    seed(&state, 2000);
    let mut order = create(&router(state).unwrap()).await;
    order.status = "verifying".into();
    let provider = ChannelEvidence {
        channel_id: format!("0x{}", "01".repeat(32)),
        peer_pubkey: order.quote.merchant_pubkey.clone(),
        funding_outpoint: format!("0x{}#0", "02".repeat(32)),
        state: "ChannelReady".into(),
        local_balance: format!("0x{:x}", 500 * SHANNONS),
        remote_balance: "0x0".into(),
        inbound_liquidity: "0x0".into(),
        observed_at: now(),
        settlement_tx_hash: None,
        state_flags: None,
        settlement: None,
    };
    order.provider_evidence = Some(provider.clone());
    assert!(evidence::ready(&order).is_err());
    let mut merchant = provider.clone();
    merchant.peer_pubkey = order.quote.provider_pubkey.clone();
    merchant.local_balance = "0x0".into();
    merchant.remote_balance = provider.local_balance.clone();
    merchant.inbound_liquidity = provider.local_balance.clone();
    order.merchant_evidence = Some(merchant.clone());
    assert!(evidence::ready(&order).is_err());
    order.probe_received = true;
    assert!(evidence::ready(&order).is_ok());
    let good = order.clone();
    order.merchant_evidence.as_mut().unwrap().observed_at = now() - 91;
    assert!(evidence::ready(&order).is_err());
    order = good.clone();
    order.merchant_evidence.as_mut().unwrap().state = "Closed".into();
    assert!(evidence::ready(&order).is_err());
    order = good.clone();
    order.merchant_evidence.as_mut().unwrap().channel_id = "different".into();
    assert!(evidence::ready(&order).is_err());
    order = good.clone();
    order.merchant_evidence.as_mut().unwrap().inbound_liquidity = format!("0x{:x}", 499 * SHANNONS);
    assert!(evidence::ready(&order).is_err());
    order = good;
    order.provider_evidence.as_mut().unwrap().local_balance = "0x1".into();
    assert!(evidence::ready(&order).is_err());
}

#[test]
fn packed_outpoint_aliases_have_one_identity() {
    let packed = format!("0x{}02000000", "ab".repeat(32));
    let text = format!("0x{}#0x2", "AB".repeat(32));
    assert_eq!(
        chain::canonical_outpoint(&packed).unwrap(),
        chain::canonical_outpoint(&text).unwrap()
    );
    assert!(chain::canonical_outpoint("0x1234").is_err());
    let store = Db::open(std::path::Path::new(":memory:")).unwrap();
    store
        .transaction(|tx| db::bind(tx, "funding_outpoint", &packed, "first"))
        .unwrap();
    assert!(
        store
            .transaction(|tx| db::bind_outpoint(tx, &chain::canonical_outpoint(&text)?, "second"))
            .is_err()
    );
    assert!(chain::canonical_outpoint(&format!("0x{}#4294967296", "ab".repeat(32))).is_err());
}

#[test]
fn durable_uniqueness_and_atomic_rollback_survive_reopening() {
    let path = std::env::temp_dir().join(format!("liquidlane-{}.sqlite3", uuid::Uuid::new_v4()));
    {
        let store = Db::open(&path).unwrap();
        store
            .transaction(|tx| {
                db::put(tx, "journal", "order", "local", &json!({"attempted":true}))?;
                db::bind(tx, "fee", "tx", "order")
            })
            .unwrap();
        let result: anyhow::Result<()> = store.transaction(|tx| {
            db::put(tx, "journal", "order", "local", &json!({"attempted":false}))?;
            db::bind(tx, "fee", "tx", "another")
        });
        assert!(result.is_err());
    }
    {
        let store = Db::open(&path).unwrap();
        assert_eq!(
            store.get::<serde_json::Value>("journal", "order").unwrap()["attempted"],
            true
        );
        assert!(
            store
                .transaction(|tx| db::bind(tx, "fee", "tx", "another"))
                .is_err()
        );
        assert!(
            store
                .transaction(|tx| db::bind(tx, "fee", "tx", "order"))
                .is_ok()
        );
    }
    std::fs::remove_file(path).unwrap();
}

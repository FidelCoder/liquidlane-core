use super::*;

async fn spender(
    client: &reqwest::Client,
    url: &str,
    point: &str,
    lock: &Value,
) -> Result<Option<(String, Value)>> {
    let (hash, index) = chain::parse_outpoint(point)?;
    let live = chain::rpc(
        client,
        url,
        "get_live_cell",
        json!([{"tx_hash":hash,"index":format!("0x{index:x}")},false]),
    )
    .await?;
    if live["status"] == "live" {
        return Ok(None);
    }
    let mut cursor = Value::Null;
    for _ in 0..20 {
        let mut params = json!([{"script":lock,"script_type":"lock",
            "script_search_mode":"exact","group_by_transaction":false},"asc","0x64"]);
        if !cursor.is_null() {
            params.as_array_mut().unwrap().push(cursor.clone());
        }
        let page = chain::rpc(client, url, "get_transactions", params).await?;
        let objects = page["objects"]
            .as_array()
            .context("settlement indexer unavailable")?;
        let mut seen = BTreeSet::new();
        for entry in objects.iter().filter(|entry| entry["io_type"] == "input") {
            let hash = entry["tx_hash"]
                .as_str()
                .context("indexer transaction hash missing")?;
            if !seen.insert(hash) {
                continue;
            }
            let tx = transaction(client, url, hash).await?;
            if input_outpoints(&tx)?.iter().any(|input| input == point) {
                return Ok(Some((hash.into(), tx)));
            }
        }
        if objects.len() < 100 {
            return Ok(None);
        }
        let next = page["last_cursor"].clone();
        ensure!(
            next.is_string() && next != cursor,
            "settlement indexer cursor did not advance"
        );
        cursor = next;
    }
    anyhow::bail!("settlement indexer scan limit reached")
}

/// Discover the actual funding spend, even when Fiber has no shutdown hash or
/// points at a different local commitment. All queries are read-only and bounded.
pub async fn inspect(
    client: &reqwest::Client,
    url: &str,
    funding: &str,
) -> Result<Option<Settlement>> {
    tokio::time::timeout(std::time::Duration::from_secs(25), async {
        chain::verify_network(client, url).await?;
        let point = chain::canonical_outpoint(funding)?;
        let (hash, index) = chain::parse_outpoint(&point)?;
        let root = transaction(client, url, &hash).await?;
        // Validate the root before using its script to query the indexer.
        summarize(&point, &root, &BTreeMap::new())?;
        let mut queue = vec![(point.clone(), output(&root, index)?["lock"].clone())];
        let mut visited = BTreeSet::new();
        let mut transactions = BTreeMap::new();
        while let Some((point, lock)) = queue.pop() {
            if !visited.insert(point.clone()) {
                continue;
            }
            ensure!(visited.len() <= 64, "settlement output scan limit reached");
            let Some((hash, tx)) = spender(client, url, &point, &lock).await? else {
                continue;
            };
            if transactions.contains_key(&hash) {
                continue;
            }
            ensure!(
                transactions.len() < MAX_TRANSACTIONS,
                "settlement transaction scan limit reached"
            );
            for (index, cell) in tx["outputs"]
                .as_array()
                .context("settlement outputs missing")?
                .iter()
                .enumerate()
            {
                if contract(cell, COMMITMENT_CODE_HASH) && tx["outputs_data"][index] == "0x" {
                    queue.push((format!("{hash}#{index}"), cell["lock"].clone()));
                }
            }
            transactions.insert(hash, tx);
        }
        summarize(&point, &root, &transactions)
    })
    .await
    .context("settlement check timed out")?
}

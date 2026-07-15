use std::time::Duration;

use anyhow::{Result, anyhow};

use super::AppStore;

const COMMIT_ATTEMPTS: usize = 30;
const COMMIT_POLL_INTERVAL: Duration = Duration::from_secs(2);

impl AppStore {
    pub(super) async fn wait_for_ckb_commitment(&self, tx_hash: &str) -> Result<()> {
        let Some(client) = self.ckb_rpc.as_ref() else {
            return Ok(());
        };

        for attempt in 0..COMMIT_ATTEMPTS {
            let transaction = client.verify_transaction(tx_hash).await?;
            if transaction.status == "committed" {
                tracing::info!(tx_hash, "CKB transaction committed before Fiber handoff");
                return Ok(());
            }
            if attempt + 1 < COMMIT_ATTEMPTS {
                tokio::time::sleep(COMMIT_POLL_INTERVAL).await;
            }
        }

        Err(anyhow!(
            "CKB capacity request did not commit within 60 seconds; Fiber handoff was not started"
        ))
    }
}

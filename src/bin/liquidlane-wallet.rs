#[tokio::main]
async fn main() -> anyhow::Result<()> {
    liquidlane_core::wallet::main().await
}

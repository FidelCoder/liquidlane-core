#[tokio::main]
async fn main() -> anyhow::Result<()> {
    liquidlane_core::connector::cli::main().await
}

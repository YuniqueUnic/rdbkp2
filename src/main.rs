#[tokio::main]
async fn main() -> anyhow::Result<()> {
    rdbkp2::run().await
}

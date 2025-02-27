use anyhow::Result;
use rdbkp2::{init_log, run};

#[tokio::main]
async fn main() -> Result<()> {
    init_log()?;
    run().await?;

    Ok(())
}

use anyhow::Result;
use rdbkp2::{init_log, load_config, run};

#[tokio::main]
async fn main() -> Result<()> {
    init_log()?;
    load_config()?;

    run().await?;

    Ok(())
}

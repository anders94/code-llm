mod api;
mod cli;
mod context;
mod diff;
mod utils;

use anyhow::Result;
use cli::run_cli;

#[tokio::main]
async fn main() -> Result<()> {
    run_cli().await
}

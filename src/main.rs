mod api;
mod cli;
mod context;
mod diff;
mod utils;

use anyhow::Result;
use cli::run_cli;
use colored;

#[tokio::main]
async fn main() -> Result<()> {
    // Enable colors globally, regardless of whether stdout is a terminal
    colored::control::set_override(true);

    run_cli().await
}

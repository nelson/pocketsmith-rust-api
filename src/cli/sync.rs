//! `pocketsmith sync` — pull PocketSmith data into the local SQLite mirror.

use anyhow::{Context, Result};

use pocketsmith_sync::client::PocketSmithClient;
use pocketsmith_sync::db;

pub fn run(_args: &[String]) -> Result<()> {
    let api_key = std::env::var("POCKETSMITH_API_KEY")
        .context("POCKETSMITH_API_KEY environment variable not set")?;

    let client = PocketSmithClient::new(api_key);
    let conn = db::open_app_db()?;

    pocketsmith_sync::sync::pull(&client, &conn)
}

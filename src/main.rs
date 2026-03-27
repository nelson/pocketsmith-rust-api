use anyhow::{Context, Result};

use pocketsmith_sync::client::PocketSmithClient;
use pocketsmith_sync::db;

fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    let api_key = std::env::var("POCKETSMITH_API_KEY")
        .context("POCKETSMITH_API_KEY environment variable not set")?;

    let client = PocketSmithClient::new(api_key);
    let conn = db::initialize("pocketsmith.db")?;

    pocketsmith_sync::sync::pull(&client, &conn)
}

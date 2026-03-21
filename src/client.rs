use anyhow::{Context, Result};
use reqwest::blocking::Client;

use crate::models::*;

const BASE_URL: &str = "https://api.pocketsmith.com/v2";

pub struct PocketSmithClient {
    http: Client,
    api_key: String,
}

impl PocketSmithClient {
    pub fn new(api_key: String) -> Self {
        Self {
            http: Client::new(),
            api_key,
        }
    }

    fn get<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T> {
        let url = format!("{}{}", BASE_URL, path);
        let resp = self
            .http
            .get(&url)
            .header("X-Developer-Key", &self.api_key)
            .header("Accept", "application/json")
            .send()
            .with_context(|| format!("GET {}", url))?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().unwrap_or_default();
            anyhow::bail!("GET {} returned {}: {}", url, status, body);
        }

        let resp_text = resp.text()?;
        serde_json::from_str(&resp_text)
            .with_context(|| format!("Failed to parse response from GET {}", url))
    }

    pub fn get_me(&self) -> Result<User> {
        self.get("/me")
    }

    pub fn get_user(&self, id: i64) -> Result<User> {
        self.get(&format!("/users/{}", id))
    }

    pub fn get_accounts(&self, user_id: i64) -> Result<Vec<Account>> {
        self.get(&format!("/users/{}/accounts", user_id))
    }

    pub fn get_transaction_accounts(&self, user_id: i64) -> Result<Vec<TransactionAccount>> {
        self.get(&format!("/users/{}/transaction_accounts", user_id))
    }

    pub fn get_categories(&self, user_id: i64) -> Result<Vec<Category>> {
        self.get(&format!("/users/{}/categories", user_id))
    }

    pub fn get_transaction(&self, id: i64) -> Result<Transaction> {
        self.get(&format!("/transactions/{}", id))
    }

    pub fn get_transactions_page(
        &self,
        user_id: i64,
        params: &TransactionParams,
        page: u32,
    ) -> Result<Vec<Transaction>> {
        let mut query: Vec<(&str, String)> = vec![("page".into(), page.to_string())];

        if let Some(ref v) = params.start_date {
            query.push(("start_date", v.clone()));
        }
        if let Some(ref v) = params.end_date {
            query.push(("end_date", v.clone()));
        }
        if let Some(ref v) = params.updated_since {
            query.push(("updated_since", v.clone()));
        }
        if let Some(v) = params.uncategorised {
            query.push(("uncategorised", if v { "1" } else { "0" }.to_string()));
        }
        if let Some(ref v) = params.transaction_type {
            query.push(("type", v.clone()));
        }
        if let Some(v) = params.needs_review {
            query.push(("needs_review", if v { "1" } else { "0" }.to_string()));
        }
        if let Some(ref v) = params.search {
            query.push(("search", v.clone()));
        }

        let url = format!("{}/users/{}/transactions", BASE_URL, user_id);
        let resp = self
            .http
            .get(&url)
            .header("X-Developer-Key", &self.api_key)
            .header("Accept", "application/json")
            .query(&query)
            .send()
            .with_context(|| format!("GET {} page {}", url, page))?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().unwrap_or_default();
            // 400 with "out of bounds" means we've gone past the last page
            if status.as_u16() == 400 && body.contains("out of bounds") {
                return Ok(Vec::new());
            }
            anyhow::bail!("GET {} page {} returned {}: {}", url, page, status, body);
        }

        let resp_text = resp.text()?;
        serde_json::from_str(&resp_text)
            .with_context(|| format!("Failed to parse transactions page {}", page))
    }

    pub fn get_all_transactions(
        &self,
        user_id: i64,
        params: &TransactionParams,
    ) -> Result<Vec<Transaction>> {
        let mut all = Vec::new();
        let mut page = 1u32;

        loop {
            let batch = self.get_transactions_page(user_id, params, page)?;
            if batch.is_empty() {
                break;
            }
            all.extend(batch);
            println!("  fetched page {} ({} transactions so far)", page, all.len());
            page += 1;
        }

        Ok(all)
    }

    pub fn update_transaction(
        &self,
        id: i64,
        update: &TransactionUpdate,
    ) -> Result<Transaction> {
        let url = format!("{}/transactions/{}", BASE_URL, id);
        let resp = self
            .http
            .put(&url)
            .header("X-Developer-Key", &self.api_key)
            .header("Accept", "application/json")
            .json(update)
            .send()
            .with_context(|| format!("PUT {}", url))?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().unwrap_or_default();
            anyhow::bail!("PUT {} returned {}: {}", url, status, body);
        }

        resp.json().context("Failed to parse update response")
    }
}

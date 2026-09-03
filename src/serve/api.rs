//! Authenticated, read-only reporting API.
//!
//! This module deliberately opens a separate SQLite read-only connection for
//! every request. The API can never reuse the HTML UI's writable connection,
//! and arbitrary analytical SQL is additionally checked with SQLite's own
//! `sqlite3_stmt_readonly` result before execution.

mod mcp;
mod oauth;

use std::io::Read;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use rusqlite::types::{Value as SqlValue, ValueRef};
use rusqlite::{params, params_from_iter, Connection};
use serde::Deserialize;
use serde_json::{json, Map, Value};
use tiny_http::{Header, Method, Request, Response, StatusCode};

use pocketsmith::db;

use super::helpers::extract_param;
use super::state::AppState;

const MAX_BODY_BYTES: u64 = 64 * 1024;
const MAX_SQL_BYTES: usize = 16 * 1024;
const MAX_ROWS: usize = 500;
const MAX_RESPONSE_BYTES: usize = 1024 * 1024;
const QUERY_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug)]
pub struct Config {
    token: Option<String>,
    oauth: Option<oauth::Config>,
    pub api_only: bool,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let token = std::env::var("REPORTING_API_TOKEN")
            .ok()
            .filter(|value| !value.is_empty());
        let api_only = std::env::var("SERVE_API_ONLY")
            .ok()
            .is_some_and(|value| matches!(value.as_str(), "1" | "true" | "yes"));
        let oauth = oauth::Config::from_env()?;

        if api_only && token.is_none() && oauth.is_none() {
            bail!(
                "SERVE_API_ONLY requires REPORTING_API_TOKEN or REPORTING_OAUTH_PASSWORD"
            );
        }
        Ok(Self {
            token,
            oauth,
            api_only,
        })
    }
}

pub fn handle(mut request: Request, state: &Arc<Mutex<AppState>>, config: &Config) {
    let url = request.url().to_string();
    let (path, query) = url.split_once('?').unwrap_or((&url, ""));

    if oauth::is_route(path) {
        if let Some(oauth) = config.oauth.as_ref() {
            oauth::handle(request, path, query, oauth);
        } else {
            respond_error(request, 503, "reporting OAuth is not configured");
        }
        return;
    }

    // The contract contains no financial data and is needed while configuring
    // a client, before that client can send its bearer token.
    if request.method() == &Method::Get && path == "/api/v1/openapi.json" {
        respond_bytes(
            request,
            200,
            "application/json; charset=utf-8",
            include_bytes!("../../openapi/reporting-api.json").to_vec(),
        );
        return;
    }

    let bearer = bearer_token(&request);
    let reporting_authorized = config.token.as_deref().is_some_and(|expected_token| {
        bearer
            .as_deref()
            .is_some_and(|value| constant_time_eq(value.as_bytes(), expected_token.as_bytes()))
    });
    let oauth_authorized = path == "/mcp"
        && bearer
            .as_deref()
            .is_some_and(|value| config.oauth.as_ref().is_some_and(|oauth| oauth.authorized(value)));

    if path != "/mcp" && config.token.is_none() {
        respond_error(request, 503, "reporting API is not configured");
        return;
    }
    if path == "/mcp" && config.token.is_none() && config.oauth.is_none() {
        respond_error(request, 503, "reporting API is not configured");
        return;
    }
    if !reporting_authorized && !oauth_authorized {
        let body = serde_json::to_vec(&json!({ "error": "unauthorized" })).unwrap();
        let mut response = Response::from_data(body)
            .with_status_code(StatusCode(401))
            .with_header(json_header());
        let challenge = config
            .oauth
            .as_ref()
            .filter(|_| path == "/mcp")
            .map_or_else(
                || "Bearer".to_string(),
                |oauth| oauth.authentication_challenge(),
            );
        response.add_header(Header::from_bytes("WWW-Authenticate", challenge).unwrap());
        let _ = request.respond(response);
        return;
    }

    if path == "/mcp" {
        mcp::handle(request, state);
        return;
    }

    let result = match (request.method(), path) {
        (&Method::Get, "/api/v1/status") => with_connection(state, status),
        (&Method::Get, "/api/v1/accounts/balances") => with_connection(state, balances),
        (&Method::Get, "/api/v1/transactions") => {
            with_connection(state, |conn| transactions(conn, query))
        }
        (&Method::Post, "/api/v1/query") => {
            let body = read_limited_body(&mut request);
            body.and_then(|body| with_connection(state, |conn| analytical_query(conn, &body)))
        }
        _ => Err(ApiError::new(404, "endpoint not found")),
    };

    match result {
        Ok(value) => respond_json(request, 200, value),
        Err(error) => respond_error(request, error.status, &error.message),
    }
}

fn bearer_token(request: &Request) -> Option<String> {
    request
        .headers()
        .iter()
        .find(|header| header.field.equiv("Authorization"))
        .and_then(|header| header.value.as_str().strip_prefix("Bearer "))
        .map(str::to_string)
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0_u8, |difference, (a, b)| difference | (a ^ b))
        == 0
}

#[derive(Debug)]
struct ApiError {
    status: u16,
    message: String,
}

impl ApiError {
    fn new(status: u16, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(error: anyhow::Error) -> Self {
        Self::new(500, format!("{error:#}"))
    }
}

impl From<rusqlite::Error> for ApiError {
    fn from(error: rusqlite::Error) -> Self {
        Self::new(500, error.to_string())
    }
}

impl From<serde_json::Error> for ApiError {
    fn from(error: serde_json::Error) -> Self {
        Self::new(500, error.to_string())
    }
}

fn with_connection<T>(
    state: &Arc<Mutex<AppState>>,
    operation: impl FnOnce(&Connection) -> Result<T, ApiError>,
) -> Result<T, ApiError> {
    let path = state
        .lock()
        .map_err(|_| ApiError::new(500, "application state unavailable"))?
        .db_path
        .clone();

    match path {
        Some(path) => {
            let conn = db::open_read_only(&path).map_err(anyhow::Error::from)?;
            operation(&conn)
        }
        None => {
            let state = state
                .lock()
                .map_err(|_| ApiError::new(500, "application state unavailable"))?;
            operation(&state.conn)
        }
    }
}

fn status(conn: &Connection) -> Result<Value, ApiError> {
    let integrity: String = conn
        .query_row("PRAGMA quick_check", [], |row| row.get(0))
        .context("database integrity check failed")?;
    let transaction_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM transactions", [], |row| row.get(0))
        .context("could not count transactions")?;
    let sync = conn
        .query_row(
            "SELECT created_at, transactions_updated, \
             CAST(strftime('%s','now') - strftime('%s', created_at) AS INTEGER) \
             FROM _operations WHERE reason = 'sync' ORDER BY id DESC LIMIT 1",
            [],
            |row| {
                Ok(json!({
                    "completed_at": row.get::<_, String>(0)?,
                    "transactions_updated": row.get::<_, i64>(1)?,
                    "age_seconds": row.get::<_, i64>(2)?,
                }))
            },
        )
        .ok();
    let fresh = sync
        .as_ref()
        .and_then(|value| value["age_seconds"].as_i64())
        .is_some_and(|age| age <= 36 * 60 * 60);

    Ok(json!({
        "status": if integrity == "ok" { "ok" } else { "degraded" },
        "database": {
            "integrity": integrity,
            "transaction_count": transaction_count,
        },
        "sync": sync,
        "data_fresh": fresh,
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

fn balances(conn: &Connection) -> Result<Value, ApiError> {
    let mut stmt = conn.prepare(
        "SELECT id, name, currency_code, account_type, current_balance, \
         current_balance_date, current_balance_in_base_currency, safe_balance, \
         safe_balance_in_base_currency, updated_at \
         FROM transaction_accounts ORDER BY name, id",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(json!({
            "id": row.get::<_, i64>(0)?,
            "name": row.get::<_, Option<String>>(1)?,
            "currency_code": row.get::<_, Option<String>>(2)?,
            "account_type": row.get::<_, Option<String>>(3)?,
            "current_balance": row.get::<_, Option<f64>>(4)?,
            "current_balance_date": row.get::<_, Option<String>>(5)?,
            "current_balance_in_base_currency": row.get::<_, Option<f64>>(6)?,
            "safe_balance": row.get::<_, Option<f64>>(7)?,
            "safe_balance_in_base_currency": row.get::<_, Option<f64>>(8)?,
            "updated_at": row.get::<_, Option<String>>(9)?,
        }))
    })?;
    let accounts = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(json!({ "accounts": accounts }))
}

fn transactions(conn: &Connection, query: &str) -> Result<Value, ApiError> {
    let from = extract_param(query, "from").filter(|value| !value.is_empty());
    let to = extract_param(query, "to").filter(|value| !value.is_empty());
    for value in [&from, &to].into_iter().flatten() {
        if !valid_date(value) {
            return Err(ApiError::new(400, "dates must use YYYY-MM-DD"));
        }
    }
    let account_id = parse_optional_i64(query, "account_id")?;
    let category_id = parse_optional_i64(query, "category_id")?;
    let limit = extract_param(query, "limit")
        .map(|value| value.parse::<usize>())
        .transpose()
        .map_err(|_| ApiError::new(400, "limit must be an integer"))?
        .unwrap_or(100)
        .clamp(1, MAX_ROWS);

    let mut stmt = conn.prepare(
        "SELECT t.id, t.date, t.payee, t.original_payee, t.amount, \
         t.amount_in_base_currency, t.is_transfer, t.category_id, c.title, \
         t.transaction_account_id, a.name, a.currency_code, t.labels, \
         t.status, t.needs_review, t.updated_at \
         FROM transactions t \
         LEFT JOIN categories c ON c.id = t.category_id \
         LEFT JOIN transaction_accounts a ON a.id = t.transaction_account_id \
         WHERE (?1 IS NULL OR t.date >= ?1) AND (?2 IS NULL OR t.date <= ?2) \
           AND (?3 IS NULL OR t.transaction_account_id = ?3) \
           AND (?4 IS NULL OR t.category_id = ?4) \
         ORDER BY t.date DESC, t.id DESC LIMIT ?5",
    )?;
    let rows = stmt.query_map(
        params![from, to, account_id, category_id, limit as i64],
        |row| {
            Ok(json!({
                "id": row.get::<_, i64>(0)?,
                "date": row.get::<_, Option<String>>(1)?,
                "payee": row.get::<_, Option<String>>(2)?,
                "original_payee": row.get::<_, Option<String>>(3)?,
                "amount": row.get::<_, Option<f64>>(4)?,
                "amount_in_base_currency": row.get::<_, Option<f64>>(5)?,
                "is_transfer": row.get::<_, Option<bool>>(6)?,
                "category": {
                    "id": row.get::<_, Option<i64>>(7)?,
                    "title": row.get::<_, Option<String>>(8)?,
                },
                "account": {
                    "id": row.get::<_, Option<i64>>(9)?,
                    "name": row.get::<_, Option<String>>(10)?,
                    "currency_code": row.get::<_, Option<String>>(11)?,
                },
                "labels": row.get::<_, Option<String>>(12)?,
                "status": row.get::<_, Option<String>>(13)?,
                "needs_review": row.get::<_, Option<bool>>(14)?,
                "updated_at": row.get::<_, Option<String>>(15)?,
            }))
        },
    )?;
    let transactions = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(json!({
        "filters": { "from": from, "to": to, "account_id": account_id, "category_id": category_id },
        "limit": limit,
        "transactions": transactions,
    }))
}

fn valid_date(value: &str) -> bool {
    value.len() == 10
        && value.as_bytes()[4] == b'-'
        && value.as_bytes()[7] == b'-'
        && value
            .bytes()
            .enumerate()
            .all(|(index, byte)| matches!(index, 4 | 7) || byte.is_ascii_digit())
}

fn parse_optional_i64(query: &str, key: &str) -> Result<Option<i64>, ApiError> {
    extract_param(query, key)
        .filter(|value| !value.is_empty())
        .map(|value| value.parse::<i64>())
        .transpose()
        .map_err(|_| ApiError::new(400, format!("{key} must be an integer")))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct QueryRequest {
    sql: String,
    #[serde(default)]
    params: Vec<Value>,
}

fn analytical_query(conn: &Connection, body: &[u8]) -> Result<Value, ApiError> {
    let request: QueryRequest = serde_json::from_slice(body)
        .map_err(|error| ApiError::new(400, format!("invalid JSON request: {error}")))?;
    let sql = request.sql.trim();
    if sql.is_empty() || sql.len() > MAX_SQL_BYTES {
        return Err(ApiError::new(400, "SQL must contain 1 to 16384 bytes"));
    }
    if sql.trim_end_matches(';').contains(';') {
        return Err(ApiError::new(400, "only one SQL statement is allowed"));
    }
    let lower = sql.to_ascii_lowercase();
    if !(lower.starts_with("select ") || lower.starts_with("select\n") || lower.starts_with("with ")) {
        return Err(ApiError::new(400, "only SELECT queries and CTEs are allowed"));
    }

    let sql_params = request
        .params
        .iter()
        .map(json_to_sql)
        .collect::<Result<Vec<_>, _>>()?;
    let started = Instant::now();
    conn.progress_handler(1_000, Some(move || started.elapsed() > QUERY_TIMEOUT));

    let mut stmt = conn.prepare(sql).map_err(|error| ApiError::new(400, error.to_string()))?;
    if !stmt.readonly() {
        return Err(ApiError::new(400, "query is not read-only"));
    }
    let columns = stmt
        .column_names()
        .into_iter()
        .map(str::to_string)
        .collect::<Vec<_>>();
    let mut cursor = stmt
        .query(params_from_iter(sql_params.iter()))
        .map_err(|error| ApiError::new(400, error.to_string()))?;
    let mut rows = Vec::new();
    let mut truncated = false;
    while let Some(row) = cursor.next().map_err(|error| ApiError::new(400, error.to_string()))? {
        if rows.len() == MAX_ROWS {
            truncated = true;
            break;
        }
        let mut values = Vec::with_capacity(columns.len());
        for index in 0..columns.len() {
            values.push(sql_to_json(row.get_ref(index)?));
        }
        rows.push(Value::Array(values));
    }

    let response = json!({ "columns": columns, "rows": rows, "truncated": truncated });
    if serde_json::to_vec(&response)?.len() > MAX_RESPONSE_BYTES {
        return Err(ApiError::new(413, "query response exceeds 1 MiB"));
    }
    Ok(response)
}

fn json_to_sql(value: &Value) -> Result<SqlValue, ApiError> {
    match value {
        Value::Null => Ok(SqlValue::Null),
        Value::Bool(value) => Ok(SqlValue::Integer(i64::from(*value))),
        Value::Number(value) => value
            .as_i64()
            .map(SqlValue::Integer)
            .or_else(|| value.as_f64().map(SqlValue::Real))
            .ok_or_else(|| ApiError::new(400, "numeric parameter is out of range")),
        Value::String(value) => Ok(SqlValue::Text(value.clone())),
        Value::Array(_) | Value::Object(_) => {
            Err(ApiError::new(400, "query parameters must be scalar values"))
        }
    }
}

fn sql_to_json(value: ValueRef<'_>) -> Value {
    match value {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(value) => json!(value),
        ValueRef::Real(value) => json!(value),
        ValueRef::Text(value) => json!(String::from_utf8_lossy(value)),
        ValueRef::Blob(value) => json!({ "hex": hex(value) }),
    }
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(DIGITS[(byte >> 4) as usize] as char);
        encoded.push(DIGITS[(byte & 0x0f) as usize] as char);
    }
    encoded
}

fn read_limited_body(request: &mut Request) -> Result<Vec<u8>, ApiError> {
    let mut body = Vec::new();
    request
        .as_reader()
        .take(MAX_BODY_BYTES + 1)
        .read_to_end(&mut body)
        .map_err(|error| ApiError::new(400, format!("could not read request body: {error}")))?;
    if body.len() as u64 > MAX_BODY_BYTES {
        return Err(ApiError::new(413, "request body exceeds 64 KiB"));
    }
    Ok(body)
}

fn respond_json(request: Request, status: u16, value: Value) {
    match serde_json::to_vec(&value) {
        Ok(body) => respond_bytes(request, status, "application/json; charset=utf-8", body),
        Err(_) => respond_error(request, 500, "could not encode response"),
    }
}

fn respond_error(request: Request, status: u16, message: &str) {
    let mut body = Map::new();
    body.insert("error".to_string(), Value::String(message.to_string()));
    respond_json(request, status, Value::Object(body));
}

fn respond_bytes(request: Request, status: u16, content_type: &str, body: Vec<u8>) {
    let response = Response::from_data(body)
        .with_status_code(StatusCode(status))
        .with_header(Header::from_bytes("Content-Type", content_type).unwrap())
        .with_header(Header::from_bytes("Cache-Control", "no-store").unwrap())
        .with_header(Header::from_bytes("X-Content-Type-Options", "nosniff").unwrap());
    let _ = request.respond(response);
}

fn json_header() -> Header {
    Header::from_bytes("Content-Type", "application/json; charset=utf-8").unwrap()
}

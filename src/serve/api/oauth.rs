//! Single-user OAuth 2.1 authorization server for ChatGPT MCP access.
//!
//! OAuth state is deliberately kept in a separate SQLite database. The
//! financial database remains read-only to every reporting and MCP request.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context, Result};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use ring::digest::{digest, SHA256};
use ring::rand::{SecureRandom, SystemRandom};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Deserialize;
use serde_json::{json, Value};
use tiny_http::{Header, Method, Request, Response, StatusCode};

const READ_SCOPE: &str = "reporting:read";
const AUTH_CODE_LIFETIME_SECONDS: i64 = 5 * 60;
const ACCESS_TOKEN_LIFETIME_SECONDS: i64 = 60 * 60;
const REFRESH_TOKEN_LIFETIME_SECONDS: i64 = 30 * 24 * 60 * 60;
const PROTECTED_RESOURCE_METADATA_PATH: &str =
    "/.well-known/oauth-protected-resource/api/v1/mcp";
const LEGACY_MCP_PROTECTED_RESOURCE_METADATA_PATH: &str =
    "/.well-known/oauth-protected-resource/mcp";
const LEGACY_PROTECTED_RESOURCE_METADATA_PATH: &str =
    "/.well-known/oauth-protected-resource";

#[derive(Debug)]
pub(super) struct Config {
    password: String,
    base_url: String,
    resource: String,
    db_path: PathBuf,
}

#[derive(Debug, Deserialize)]
struct RegistrationRequest {
    redirect_uris: Vec<String>,
    #[serde(default)]
    client_name: Option<String>,
    #[serde(default)]
    token_endpoint_auth_method: Option<String>,
    #[serde(default)]
    grant_types: Option<Vec<String>>,
    #[serde(default)]
    response_types: Option<Vec<String>>,
}

#[derive(Debug)]
struct AuthorizationRequest {
    client_id: String,
    redirect_uri: String,
    state: String,
    code_challenge: String,
    scope: String,
    resource: String,
}

impl Config {
    pub(super) fn from_env() -> Result<Option<Self>> {
        let Some(password) = std::env::var("REPORTING_OAUTH_PASSWORD")
            .ok()
            .filter(|value| !value.is_empty())
        else {
            return Ok(None);
        };
        if password.len() < 20 {
            bail!("REPORTING_OAUTH_PASSWORD must contain at least 20 characters");
        }

        let base_url = std::env::var("REPORTING_BASE_URL")
            .context("REPORTING_OAUTH_PASSWORD requires REPORTING_BASE_URL")?
            .trim_end_matches('/')
            .to_string();
        let parsed_base = reqwest::Url::parse(&base_url)
            .context("REPORTING_BASE_URL must be a valid URL")?;
        if parsed_base.scheme() != "https"
            || parsed_base.host_str().is_none()
            || parsed_base.path() != "/"
            || parsed_base.query().is_some()
            || parsed_base.fragment().is_some()
            || !parsed_base.username().is_empty()
            || parsed_base.password().is_some()
        {
            bail!("REPORTING_BASE_URL must be an HTTPS origin without a path");
        }
        let db_path = std::env::var("REPORTING_OAUTH_DB")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("/data/reporting-oauth.db"));

        let config = Self {
            password,
            resource: format!("{base_url}{}", super::MCP_PATH),
            base_url,
            db_path,
        };
        config.initialize()?;
        Ok(Some(config))
    }

    fn initialize(&self) -> Result<()> {
        if let Some(parent) = self.db_path.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("could not create OAuth database directory {}", parent.display())
            })?;
        }
        #[cfg(unix)]
        {
            use std::fs::OpenOptions;
            use std::os::unix::fs::OpenOptionsExt;
            match OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&self.db_path)
            {
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error).context("could not create OAuth database"),
            }
        }
        let conn = self.connection()?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS oauth_clients (
               client_id TEXT PRIMARY KEY,
               redirect_uris TEXT NOT NULL,
               client_name TEXT,
               created_at INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS oauth_codes (
               code_hash BLOB PRIMARY KEY,
               client_id TEXT NOT NULL,
               redirect_uri TEXT NOT NULL,
               code_challenge TEXT NOT NULL,
               scope TEXT NOT NULL,
               expires_at INTEGER NOT NULL,
               used INTEGER NOT NULL DEFAULT 0
             );
             CREATE TABLE IF NOT EXISTS oauth_access_tokens (
               token_hash BLOB PRIMARY KEY,
               client_id TEXT NOT NULL,
               scope TEXT NOT NULL,
               expires_at INTEGER NOT NULL,
               created_at INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS oauth_refresh_tokens (
               token_hash BLOB PRIMARY KEY,
               client_id TEXT NOT NULL,
               scope TEXT NOT NULL,
               expires_at INTEGER NOT NULL,
               revoked INTEGER NOT NULL DEFAULT 0,
               created_at INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS oauth_login_attempts (
               attempted_at INTEGER NOT NULL
             );
             DELETE FROM oauth_codes WHERE expires_at <= unixepoch() OR used = 1;
             DELETE FROM oauth_access_tokens WHERE expires_at <= unixepoch();
             DELETE FROM oauth_refresh_tokens WHERE expires_at <= unixepoch() OR revoked = 1;
             DELETE FROM oauth_login_attempts WHERE attempted_at <= unixepoch() - 300;",
        )?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&self.db_path, std::fs::Permissions::from_mode(0o600))?;
        }
        Ok(())
    }

    fn connection(&self) -> Result<Connection> {
        let conn = Connection::open(&self.db_path)
            .with_context(|| format!("could not open OAuth database {}", self.db_path.display()))?;
        conn.busy_timeout(std::time::Duration::from_secs(2))?;
        Ok(conn)
    }

    pub(super) fn authorized(&self, token: &str) -> bool {
        let Ok(conn) = self.connection() else {
            return false;
        };
        conn.query_row(
            "SELECT 1 FROM oauth_access_tokens
             WHERE token_hash = ?1 AND expires_at > ?2 AND scope = ?3",
            params![token_hash(token), now(), READ_SCOPE],
            |_| Ok(()),
        )
        .is_ok()
    }

    pub(super) fn authentication_challenge(&self) -> String {
        format!(
            "Bearer resource_metadata=\"{}{PROTECTED_RESOURCE_METADATA_PATH}\", scope=\"{}\"",
            self.base_url, READ_SCOPE,
        )
    }
}

pub(super) fn is_route(path: &str) -> bool {
    matches!(
        path,
        PROTECTED_RESOURCE_METADATA_PATH
            | LEGACY_MCP_PROTECTED_RESOURCE_METADATA_PATH
            | LEGACY_PROTECTED_RESOURCE_METADATA_PATH
            | "/.well-known/oauth-authorization-server"
            | "/oauth/register"
            | "/oauth/authorize"
            | "/oauth/token"
    )
}

pub(super) fn handle(mut request: Request, path: &str, query: &str, config: &Config) {
    match (request.method(), path) {
        (&Method::Get, PROTECTED_RESOURCE_METADATA_PATH)
        | (&Method::Get, LEGACY_MCP_PROTECTED_RESOURCE_METADATA_PATH)
        | (&Method::Get, LEGACY_PROTECTED_RESOURCE_METADATA_PATH) => respond_json(
            request,
            200,
            json!({
                "resource": config.resource,
                "authorization_servers": [config.base_url],
                "scopes_supported": [READ_SCOPE],
                "bearer_methods_supported": ["header"]
            }),
        ),
        (&Method::Get, "/.well-known/oauth-authorization-server") => respond_json(
            request,
            200,
            json!({
                "issuer": config.base_url,
                "authorization_endpoint": format!("{}/oauth/authorize", config.base_url),
                "token_endpoint": format!("{}/oauth/token", config.base_url),
                "registration_endpoint": format!("{}/oauth/register", config.base_url),
                "response_types_supported": ["code"],
                "grant_types_supported": ["authorization_code", "refresh_token"],
                "code_challenge_methods_supported": ["S256"],
                "token_endpoint_auth_methods_supported": ["none"],
                "authorization_response_iss_parameter_supported": true,
                "scopes_supported": [READ_SCOPE]
            }),
        ),
        (&Method::Post, "/oauth/register") => match read_json::<RegistrationRequest>(&mut request)
            .and_then(|registration| register_client(config, registration))
        {
            Ok(value) => respond_json(request, 201, value),
            Err(error) => respond_oauth_error(request, 400, "invalid_client_metadata", &error),
        },
        (&Method::Get, "/oauth/authorize") => match parse_authorization(config, query) {
            Ok(authorization) => respond_authorization_form(request, &authorization, None, 200),
            Err(error) => respond_oauth_error(request, 400, "invalid_request", &error),
        },
        (&Method::Post, "/oauth/authorize") => {
            let result = super::read_limited_body(&mut request)
                .map_err(|error| anyhow::anyhow!(error.message))
                .and_then(|body| parse_form(&body))
                .and_then(|form| approve_authorization(config, form));
            match result {
                Ok(redirect) => respond_redirect(request, &redirect),
                Err(error) => respond_html_error(request, 400, &error),
            }
        }
        (&Method::Post, "/oauth/token") => {
            let result = super::read_limited_body(&mut request)
                .map_err(|error| anyhow::anyhow!(error.message))
                .and_then(|body| parse_form(&body))
                .and_then(|form| exchange_token(config, &form));
            match result {
                Ok(value) => respond_json(request, 200, value),
                Err(error) => respond_oauth_error(request, 400, "invalid_grant", &error),
            }
        }
        _ => respond_oauth_error(request, 405, "invalid_request", "method not allowed"),
    }
}

fn register_client(config: &Config, registration: RegistrationRequest) -> Result<Value> {
    if registration.redirect_uris.is_empty() || registration.redirect_uris.len() > 10 {
        bail!("redirect_uris must contain between one and ten values");
    }
    if registration
        .redirect_uris
        .iter()
        .any(|uri| !valid_redirect_uri(uri))
    {
        bail!("redirect URIs must be HTTPS URLs without fragments");
    }
    if registration.token_endpoint_auth_method.as_deref().unwrap_or("none") != "none" {
        bail!("only public clients using token_endpoint_auth_method=none are supported");
    }
    if registration
        .grant_types
        .as_ref()
        .is_some_and(|values| {
            values
                .iter()
                .any(|value| value != "authorization_code" && value != "refresh_token")
        })
    {
        bail!("unsupported grant type");
    }
    if registration
        .response_types
        .as_ref()
        .is_some_and(|values| values.iter().any(|value| value != "code"))
    {
        bail!("unsupported response type");
    }

    let client_id = format!("ps_{}", random_token()?);
    let redirect_uris = serde_json::to_string(&registration.redirect_uris)?;
    config.connection()?.execute(
        "INSERT INTO oauth_clients (client_id, redirect_uris, client_name, created_at)
         VALUES (?1, ?2, ?3, ?4)",
        params![client_id, redirect_uris, registration.client_name, now()],
    )?;
    Ok(json!({
        "client_id": client_id,
        "client_id_issued_at": now(),
        "redirect_uris": registration.redirect_uris,
        "client_name": registration.client_name,
        "token_endpoint_auth_method": "none",
        "grant_types": ["authorization_code", "refresh_token"],
        "response_types": ["code"]
    }))
}

fn parse_authorization(config: &Config, encoded: &str) -> Result<AuthorizationRequest> {
    let form = parse_form(encoded.as_bytes())?;
    authorization_from_form(config, &form)
}

fn authorization_from_form(
    config: &Config,
    form: &HashMap<String, String>,
) -> Result<AuthorizationRequest> {
    if required(form, "response_type")? != "code" {
        bail!("response_type must be code");
    }
    if required(form, "code_challenge_method")? != "S256" {
        bail!("code_challenge_method must be S256");
    }
    let client_id = required(form, "client_id")?.to_string();
    let redirect_uri = required(form, "redirect_uri")?.to_string();
    let state = required(form, "state")?.to_string();
    let code_challenge = required(form, "code_challenge")?.to_string();
    if code_challenge.len() < 43 || code_challenge.len() > 128 {
        bail!("invalid code_challenge");
    }
    let resource = required(form, "resource")?.to_string();
    if resource != config.resource {
        bail!("resource does not identify this MCP server");
    }
    let scope = form
        .get("scope")
        .map(String::as_str)
        .unwrap_or(READ_SCOPE)
        .to_string();
    if scope != READ_SCOPE {
        bail!("only reporting:read is supported");
    }

    let stored: Option<String> = config
        .connection()?
        .query_row(
            "SELECT redirect_uris FROM oauth_clients WHERE client_id = ?1",
            [&client_id],
            |row| row.get(0),
        )
        .optional()?;
    let Some(stored) = stored else {
        bail!("unknown client_id");
    };
    let registered: Vec<String> = serde_json::from_str(&stored)?;
    if !registered.iter().any(|uri| uri == &redirect_uri) {
        bail!("redirect_uri is not registered for this client");
    }

    Ok(AuthorizationRequest {
        client_id,
        redirect_uri,
        state,
        code_challenge,
        scope,
        resource,
    })
}

fn approve_authorization(config: &Config, form: HashMap<String, String>) -> Result<String> {
    let authorization = authorization_from_form(config, &form)?;
    let password = required(&form, "password")?;
    check_login_rate(config)?;
    if !super::constant_time_eq(&token_hash(password), &token_hash(&config.password)) {
        record_failed_login(config)?;
        bail!("authorization password was not accepted");
    }
    clear_failed_logins(config)?;

    let code = random_token()?;
    config.connection()?.execute(
        "INSERT INTO oauth_codes
         (code_hash, client_id, redirect_uri, code_challenge, scope, expires_at, used)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0)",
        params![
            token_hash(&code),
            authorization.client_id,
            authorization.redirect_uri,
            authorization.code_challenge,
            authorization.scope,
            now() + AUTH_CODE_LIFETIME_SECONDS
        ],
    )?;

    let separator = if authorization.redirect_uri.contains('?') { '&' } else { '?' };
    Ok(format!(
        "{}{separator}code={}&state={}&iss={}",
        authorization.redirect_uri,
        percent_encode(&code),
        percent_encode(&authorization.state),
        percent_encode(&config.base_url)
    ))
}

fn exchange_token(config: &Config, form: &HashMap<String, String>) -> Result<Value> {
    match required(form, "grant_type")? {
        "authorization_code" => exchange_authorization_code(config, form),
        "refresh_token" => exchange_refresh_token(config, form),
        _ => bail!("unsupported grant_type"),
    }
}

fn exchange_authorization_code(config: &Config, form: &HashMap<String, String>) -> Result<Value> {
    let code = required(form, "code")?;
    let client_id = required(form, "client_id")?;
    let redirect_uri = required(form, "redirect_uri")?;
    let verifier = required(form, "code_verifier")?;
    if verifier.len() < 43 || verifier.len() > 128 {
        bail!("invalid code_verifier");
    }

    let mut conn = config.connection()?;
    let transaction = conn.transaction()?;
    let stored = transaction
        .query_row(
            "SELECT client_id, redirect_uri, code_challenge, scope, expires_at, used
             FROM oauth_codes WHERE code_hash = ?1",
            [token_hash(code)],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, bool>(5)?,
                ))
            },
        )
        .optional()?
        .context("authorization code is unknown")?;
    if stored.0 != client_id || stored.1 != redirect_uri || stored.4 <= now() || stored.5 {
        bail!("authorization code is invalid or expired");
    }
    if pkce_challenge(verifier) != stored.2 {
        bail!("PKCE verification failed");
    }
    if transaction.execute(
        "UPDATE oauth_codes SET used = 1 WHERE code_hash = ?1 AND used = 0",
        [token_hash(code)],
    )? != 1
    {
        bail!("authorization code was already used");
    }
    let response = issue_tokens(&transaction, client_id, &stored.3)?;
    transaction.commit()?;
    Ok(response)
}

fn exchange_refresh_token(config: &Config, form: &HashMap<String, String>) -> Result<Value> {
    let refresh_token = required(form, "refresh_token")?;
    let client_id = required(form, "client_id")?;
    let mut conn = config.connection()?;
    let transaction = conn.transaction()?;
    let stored = transaction
        .query_row(
            "SELECT client_id, scope, expires_at, revoked FROM oauth_refresh_tokens
             WHERE token_hash = ?1",
            [token_hash(refresh_token)],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, bool>(3)?,
                ))
            },
        )
        .optional()?
        .context("refresh token is unknown")?;
    if stored.0 != client_id || stored.2 <= now() || stored.3 {
        bail!("refresh token is invalid or expired");
    }
    if transaction.execute(
        "UPDATE oauth_refresh_tokens SET revoked = 1
         WHERE token_hash = ?1 AND revoked = 0",
        [token_hash(refresh_token)],
    )? != 1
    {
        bail!("refresh token was already used");
    }
    let response = issue_tokens(&transaction, client_id, &stored.1)?;
    transaction.commit()?;
    Ok(response)
}

fn issue_tokens(conn: &Connection, client_id: &str, scope: &str) -> Result<Value> {
    let access_token = random_token()?;
    let refresh_token = random_token()?;
    let issued_at = now();
    conn.execute(
        "INSERT INTO oauth_access_tokens (token_hash, client_id, scope, expires_at, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            token_hash(&access_token),
            client_id,
            scope,
            issued_at + ACCESS_TOKEN_LIFETIME_SECONDS,
            issued_at
        ],
    )?;
    conn.execute(
        "INSERT INTO oauth_refresh_tokens
         (token_hash, client_id, scope, expires_at, revoked, created_at)
         VALUES (?1, ?2, ?3, ?4, 0, ?5)",
        params![
            token_hash(&refresh_token),
            client_id,
            scope,
            issued_at + REFRESH_TOKEN_LIFETIME_SECONDS,
            issued_at
        ],
    )?;
    Ok(json!({
        "access_token": access_token,
        "token_type": "Bearer",
        "expires_in": ACCESS_TOKEN_LIFETIME_SECONDS,
        "refresh_token": refresh_token,
        "scope": scope
    }))
}

fn check_login_rate(config: &Config) -> Result<()> {
    let conn = config.connection()?;
    conn.execute(
        "DELETE FROM oauth_login_attempts WHERE attempted_at <= ?1",
        [now() - 5 * 60],
    )?;
    let attempts: i64 = conn.query_row("SELECT COUNT(*) FROM oauth_login_attempts", [], |row| {
        row.get(0)
    })?;
    if attempts >= 5 {
        bail!("too many failed attempts; wait five minutes");
    }
    Ok(())
}

fn record_failed_login(config: &Config) -> Result<()> {
    config.connection()?.execute(
        "INSERT INTO oauth_login_attempts (attempted_at) VALUES (?1)",
        [now()],
    )?;
    Ok(())
}

fn clear_failed_logins(config: &Config) -> Result<()> {
    config
        .connection()?
        .execute("DELETE FROM oauth_login_attempts", [])?;
    Ok(())
}

fn respond_authorization_form(
    request: Request,
    authorization: &AuthorizationRequest,
    error: Option<&str>,
    status: u16,
) {
    let error = error.map_or_else(String::new, |message| {
        format!("<p class=\"error\">{}</p>", html_escape(message))
    });
    let body = format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\
         <title>Authorize PocketSmith Reporting</title><style>\
         body{{font:16px system-ui;max-width:34rem;margin:4rem auto;padding:0 1rem;color:#17212b}}\
         form{{display:grid;gap:1rem}}input,button{{font:inherit;padding:.7rem}}\
         button{{background:#12664f;color:white;border:0;border-radius:.3rem}}\
         .error{{color:#a21b1b}}code{{overflow-wrap:anywhere}}</style></head><body>\
         <h1>Authorize PocketSmith Reporting</h1>\
         <p>Allow this client to read balances and transactions. It cannot change PocketSmith.</p>\
         <p>Callback: <code>{}</code></p>{error}\
         <form method=\"post\" action=\"/oauth/authorize\">\
         {}<label>Authorization password<input type=\"password\" name=\"password\" required autocomplete=\"current-password\"></label>\
         <button type=\"submit\">Authorize read-only access</button></form></body></html>",
        html_escape(&authorization.redirect_uri),
        authorization_fields(authorization)
    );
    respond_html_with_callback(request, status, body, Some(&authorization.redirect_uri));
}

fn authorization_fields(authorization: &AuthorizationRequest) -> String {
    let fields = [
        ("response_type", "code"),
        ("client_id", authorization.client_id.as_str()),
        ("redirect_uri", authorization.redirect_uri.as_str()),
        ("state", authorization.state.as_str()),
        ("code_challenge", authorization.code_challenge.as_str()),
        ("code_challenge_method", "S256"),
        ("scope", authorization.scope.as_str()),
        ("resource", authorization.resource.as_str()),
    ];
    fields
        .iter()
        .map(|(name, value)| {
            format!(
                "<input type=\"hidden\" name=\"{}\" value=\"{}\">",
                name,
                html_escape(value)
            )
        })
        .collect()
}

fn respond_html_error(request: Request, status: u16, error: &anyhow::Error) {
    let body = format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><title>Authorization failed</title></head>\
         <body><h1>Authorization failed</h1><p>{}</p><p>Return to ChatGPT and try connecting again.</p></body></html>",
        html_escape(&format!("{error:#}"))
    );
    respond_html(request, status, body);
}

fn respond_redirect(request: Request, location: &str) {
    // A successful form POST must become a GET to the OAuth callback. 303 is
    // explicit about that transition, unlike the historically ambiguous 302.
    let response = Response::empty(StatusCode(303))
        .with_header(Header::from_bytes("Location", location).unwrap())
        .with_header(Header::from_bytes("Cache-Control", "no-store").unwrap());
    let _ = request.respond(response);
}

fn respond_html(request: Request, status: u16, body: String) {
    respond_html_with_callback(request, status, body, None);
}

fn respond_html_with_callback(
    request: Request,
    status: u16,
    body: String,
    callback_uri: Option<&str>,
) {
    let csp = authorization_content_security_policy(callback_uri);
    let response = Response::from_string(body)
        .with_status_code(StatusCode(status))
        .with_header(Header::from_bytes("Content-Type", "text/html; charset=utf-8").unwrap())
        .with_header(Header::from_bytes("Cache-Control", "no-store").unwrap())
        .with_header(Header::from_bytes("X-Content-Type-Options", "nosniff").unwrap())
        .with_header(Header::from_bytes("Content-Security-Policy", csp).unwrap());
    let _ = request.respond(response);
}

fn authorization_content_security_policy(callback_uri: Option<&str>) -> String {
    let callback_origin = callback_uri
        .and_then(|value| reqwest::Url::parse(value).ok())
        .map(|url| url.origin().ascii_serialization())
        .filter(|origin| origin != "null");
    let form_action = callback_origin.map_or_else(
        || "'self'".to_string(),
        |origin| format!("'self' {origin}"),
    );
    format!(
        "default-src 'none'; style-src 'unsafe-inline'; form-action {form_action}; \
         frame-ancestors 'none'; base-uri 'none'"
    )
}

fn respond_json(request: Request, status: u16, value: Value) {
    let body = serde_json::to_vec(&value).unwrap_or_default();
    let response = Response::from_data(body)
        .with_status_code(StatusCode(status))
        .with_header(Header::from_bytes("Content-Type", "application/json; charset=utf-8").unwrap())
        .with_header(Header::from_bytes("Cache-Control", "no-store").unwrap())
        .with_header(Header::from_bytes("X-Content-Type-Options", "nosniff").unwrap());
    let _ = request.respond(response);
}

fn respond_oauth_error(
    request: Request,
    status: u16,
    error: &str,
    description: impl ToString,
) {
    respond_json(
        request,
        status,
        json!({ "error": error, "error_description": description.to_string() }),
    );
}

fn read_json<T: for<'de> Deserialize<'de>>(request: &mut Request) -> Result<T> {
    let body = super::read_limited_body(request).map_err(|error| anyhow::anyhow!(error.message))?;
    serde_json::from_slice(&body).context("invalid JSON request")
}

fn parse_form(body: &[u8]) -> Result<HashMap<String, String>> {
    let encoded = std::str::from_utf8(body).context("form is not UTF-8")?;
    let mut values = HashMap::new();
    for pair in encoded.split('&').filter(|pair| !pair.is_empty()) {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        values.insert(percent_decode(key)?, percent_decode(value)?);
    }
    Ok(values)
}

fn required<'a>(values: &'a HashMap<String, String>, key: &str) -> Result<&'a str> {
    values
        .get(key)
        .filter(|value| !value.is_empty())
        .map(String::as_str)
        .with_context(|| format!("missing {key}"))
}

fn valid_redirect_uri(value: &str) -> bool {
    let Ok(url) = reqwest::Url::parse(value) else {
        return false;
    };
    value.len() <= 2048
        && url.scheme() == "https"
        && url.host_str().is_some()
        && url.fragment().is_none()
        && url.username().is_empty()
        && url.password().is_none()
}

fn random_token() -> Result<String> {
    let mut bytes = [0_u8; 32];
    SystemRandom::new()
        .fill(&mut bytes)
        .map_err(|_| anyhow::anyhow!("secure random number generation failed"))?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

fn token_hash(value: &str) -> Vec<u8> {
    digest(&SHA256, value.as_bytes()).as_ref().to_vec()
}

fn pkce_challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(digest(&SHA256, verifier.as_bytes()).as_ref())
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

fn percent_decode(value: &str) -> Result<String> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'+' => decoded.push(b' '),
            b'%' if index + 2 < bytes.len() => {
                let high = hex_value(bytes[index + 1]).context("invalid percent encoding")?;
                let low = hex_value(bytes[index + 2]).context("invalid percent encoding")?;
                decoded.push((high << 4) | low);
                index += 2;
            }
            b'%' => bail!("invalid percent encoding"),
            byte => decoded.push(byte),
        }
        index += 1;
    }
    String::from_utf8(decoded).context("decoded form value is not UTF-8")
}

fn hex_value(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn percent_encode(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

fn html_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> (Config, PathBuf) {
        let path = std::env::temp_dir().join(format!(
            "pocketsmith-oauth-{}.db",
            random_token().unwrap()
        ));
        let config = Config {
            password: "correct horse battery staple".to_string(),
            base_url: "https://finance.example.com".to_string(),
            resource: "https://finance.example.com/api/v1/mcp".to_string(),
            db_path: path.clone(),
        };
        config.initialize().unwrap();
        (config, path)
    }

    fn register(config: &Config) -> String {
        register_client(
            config,
            RegistrationRequest {
                redirect_uris: vec!["https://chatgpt.com/oauth/callback".to_string()],
                client_name: Some("ChatGPT".to_string()),
                token_endpoint_auth_method: Some("none".to_string()),
                grant_types: None,
                response_types: None,
            },
        )
        .unwrap()["client_id"]
            .as_str()
            .unwrap()
            .to_string()
    }

    #[test]
    fn protected_resource_metadata_uses_the_mcp_resource_path() {
        let (config, path) = config();

        assert!(is_route(PROTECTED_RESOURCE_METADATA_PATH));
        assert!(is_route(LEGACY_MCP_PROTECTED_RESOURCE_METADATA_PATH));
        assert!(is_route(LEGACY_PROTECTED_RESOURCE_METADATA_PATH));
        assert_eq!(
            config.authentication_challenge(),
            concat!(
                "Bearer resource_metadata=\"https://finance.example.com",
                "/.well-known/oauth-protected-resource/api/v1/mcp\", ",
                "scope=\"reporting:read\""
            )
        );

        drop(config);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("db-shm"));
        let _ = std::fs::remove_file(path.with_extension("db-wal"));
    }

    #[test]
    fn authorization_csp_allows_only_the_registered_callback_origin() {
        let policy = authorization_content_security_policy(Some(
            "https://chatgpt.com/connector_platform_oauth_redirect",
        ));
        assert!(policy.contains("form-action 'self' https://chatgpt.com;"));
        assert!(!policy.contains("attacker.example"));

        let invalid = authorization_content_security_policy(Some("not a URL"));
        assert!(invalid.contains("form-action 'self';"));
    }

    #[test]
    fn authorization_code_and_refresh_flow_issue_valid_tokens() {
        let (config, path) = config();
        let client_id = register(&config);
        let verifier = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-._~";
        let mut form = HashMap::from([
            ("response_type".to_string(), "code".to_string()),
            ("client_id".to_string(), client_id.clone()),
            (
                "redirect_uri".to_string(),
                "https://chatgpt.com/oauth/callback".to_string(),
            ),
            ("state".to_string(), "state-value".to_string()),
            ("code_challenge".to_string(), pkce_challenge(verifier)),
            ("code_challenge_method".to_string(), "S256".to_string()),
            ("scope".to_string(), READ_SCOPE.to_string()),
            ("resource".to_string(), config.resource.clone()),
            ("password".to_string(), config.password.clone()),
        ]);
        let redirect = approve_authorization(&config, form.clone()).unwrap();
        let query = redirect.split_once('?').unwrap().1;
        let code = parse_form(query.as_bytes()).unwrap().remove("code").unwrap();

        form = HashMap::from([
            ("grant_type".to_string(), "authorization_code".to_string()),
            ("client_id".to_string(), client_id.clone()),
            (
                "redirect_uri".to_string(),
                "https://chatgpt.com/oauth/callback".to_string(),
            ),
            ("code".to_string(), code.clone()),
            ("code_verifier".to_string(), verifier.to_string()),
        ]);
        let tokens = exchange_token(&config, &form).unwrap();
        let access = tokens["access_token"].as_str().unwrap();
        assert!(config.authorized(access));
        assert!(exchange_token(&config, &form).is_err(), "code must be single use");

        let refresh = tokens["refresh_token"].as_str().unwrap();
        let refreshed = exchange_token(
            &config,
            &HashMap::from([
                ("grant_type".to_string(), "refresh_token".to_string()),
                ("client_id".to_string(), client_id),
                ("refresh_token".to_string(), refresh.to_string()),
            ]),
        )
        .unwrap();
        assert!(config.authorized(refreshed["access_token"].as_str().unwrap()));

        drop(config);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("db-shm"));
        let _ = std::fs::remove_file(path.with_extension("db-wal"));
    }

    #[test]
    fn rejects_unregistered_redirect_and_wrong_pkce_verifier() {
        let (config, path) = config();
        let client_id = register(&config);
        let invalid = HashMap::from([
            ("response_type".to_string(), "code".to_string()),
            ("client_id".to_string(), client_id.clone()),
            ("redirect_uri".to_string(), "https://attacker.example/callback".to_string()),
            ("state".to_string(), "state".to_string()),
            ("code_challenge".to_string(), "a".repeat(43)),
            ("code_challenge_method".to_string(), "S256".to_string()),
            ("scope".to_string(), READ_SCOPE.to_string()),
            ("resource".to_string(), config.resource.clone()),
        ]);
        assert!(authorization_from_form(&config, &invalid).is_err());

        let verifier = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-._~";
        let mut valid = invalid;
        valid.insert(
            "redirect_uri".to_string(),
            "https://chatgpt.com/oauth/callback".to_string(),
        );
        valid.insert("code_challenge".to_string(), pkce_challenge(verifier));
        valid.insert("password".to_string(), config.password.clone());
        let redirect = approve_authorization(&config, valid).unwrap();
        let code = parse_form(redirect.split_once('?').unwrap().1.as_bytes())
            .unwrap()
            .remove("code")
            .unwrap();
        let exchange = HashMap::from([
            ("grant_type".to_string(), "authorization_code".to_string()),
            ("client_id".to_string(), client_id),
            (
                "redirect_uri".to_string(),
                "https://chatgpt.com/oauth/callback".to_string(),
            ),
            ("code".to_string(), code),
            (
                "code_verifier".to_string(),
                "wrong-verifier-that-is-still-long-enough-123456789".to_string(),
            ),
        ]);
        assert!(exchange_token(&config, &exchange).is_err());

        drop(config);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("db-shm"));
        let _ = std::fs::remove_file(path.with_extension("db-wal"));
    }
}

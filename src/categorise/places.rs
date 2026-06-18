//! Google Places (New) `searchText` client + cache integration
//! (categorisation final stage).
//!
//! Network access sits behind the [`PlacesClient`] trait so the scan
//! logic and tests can run without hitting Google — tests supply canned
//! responses, exactly how the rest of the codebase keeps HTTP out of the
//! test suite. The pure [`parse_search_text`] response parser is unit
//! tested over fixed JSON bodies.
//!
//! [`lookup`] is the cache seam every caller uses: it reads
//! `place_lookups` first and only calls the network on a miss, then
//! persists the result so a re-scan costs zero API calls.

use anyhow::{Context, Result};
use rusqlite::Connection;
use serde::Deserialize;

use crate::db::place_lookups::{self, LookupStatus, PlaceLookupRow};

const SEARCH_TEXT_URL: &str = "https://places.googleapis.com/v1/places:searchText";
/// Only the cheap fields we actually consume. Keeping the mask tight keeps
/// the request in the lowest Places billing SKU.
const FIELD_MASK: &str =
    "places.id,places.displayName,places.primaryType,places.types,places.formattedAddress";

/// What a single Places lookup yields once parsed. Maps onto a
/// [`PlaceLookupRow`] for caching.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedPlace {
    pub place_id: Option<String>,
    pub display_name: Option<String>,
    pub primary_type: Option<String>,
    pub types: Vec<String>,
    pub status: LookupStatus,
}

impl ParsedPlace {
    fn no_result() -> Self {
        ParsedPlace {
            place_id: None,
            display_name: None,
            primary_type: None,
            types: vec![],
            status: LookupStatus::NoResult,
        }
    }
}

/// The network seam. The real implementation calls Google; tests provide
/// a fake that returns canned bodies.
pub trait PlacesClient {
    /// Run a `searchText` query, returning the raw JSON response body.
    fn search_text(&self, query: &str) -> Result<String>;
}

/// Live client over the Places API (New). Key read from
/// `GOOGLE_PLACES_API_KEY`.
pub struct GooglePlacesClient {
    http: reqwest::blocking::Client,
    api_key: String,
}

impl GooglePlacesClient {
    pub fn new(api_key: String) -> Self {
        Self {
            http: reqwest::blocking::Client::new(),
            api_key,
        }
    }

    /// Construct from the `GOOGLE_PLACES_API_KEY` environment variable.
    pub fn from_env() -> Result<Self> {
        let api_key = std::env::var("GOOGLE_PLACES_API_KEY")
            .context("GOOGLE_PLACES_API_KEY not set (see .env / plan §10)")?;
        Ok(Self::new(api_key))
    }
}

impl PlacesClient for GooglePlacesClient {
    fn search_text(&self, query: &str) -> Result<String> {
        let body = serde_json::json!({ "textQuery": query, "maxResultCount": 1 });
        let resp = self
            .http
            .post(SEARCH_TEXT_URL)
            .header("Content-Type", "application/json")
            .header("X-Goog-Api-Key", &self.api_key)
            .header("X-Goog-FieldMask", FIELD_MASK)
            .json(&body)
            .send()
            .with_context(|| format!("POST {SEARCH_TEXT_URL}"))?;
        let status = resp.status();
        let text = resp.text().unwrap_or_default();
        if !status.is_success() {
            anyhow::bail!("POST {SEARCH_TEXT_URL} returned {status}: {text}");
        }
        Ok(text)
    }
}

// --- response parsing (pure, unit tested) ---------------------------------

#[derive(Debug, Deserialize)]
struct SearchTextResponse {
    #[serde(default)]
    places: Vec<PlaceJson>,
}

#[derive(Debug, Deserialize)]
struct PlaceJson {
    id: Option<String>,
    #[serde(rename = "displayName")]
    display_name: Option<DisplayName>,
    #[serde(rename = "primaryType")]
    primary_type: Option<String>,
    #[serde(default)]
    types: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct DisplayName {
    text: Option<String>,
}

/// Parse a `searchText` response body into the first place's salient
/// fields. An empty `places` array yields a `NoResult`.
pub fn parse_search_text(body: &str) -> Result<ParsedPlace> {
    let parsed: SearchTextResponse =
        serde_json::from_str(body).context("parse Places searchText response")?;
    let Some(first) = parsed.places.into_iter().next() else {
        return Ok(ParsedPlace::no_result());
    };
    Ok(ParsedPlace {
        place_id: first.id,
        display_name: first.display_name.and_then(|d| d.text),
        primary_type: first.primary_type,
        types: first.types,
        status: LookupStatus::Ok,
    })
}

// --- cache seam -----------------------------------------------------------

/// Look up `query`, cache-first. Returns the cached row when present;
/// otherwise calls `client`, persists the parsed result (including
/// `no_result` / `error` outcomes, so we don't re-query a dud), and
/// returns it.
pub fn lookup(
    conn: &Connection,
    client: &dyn PlacesClient,
    query: &str,
) -> Result<PlaceLookupRow> {
    if let Some(row) = place_lookups::get(conn, query)? {
        return Ok(row);
    }

    let row = match client.search_text(query) {
        Ok(body) => {
            let parsed = parse_search_text(&body).unwrap_or_else(|_| ParsedPlace {
                status: LookupStatus::Error,
                ..ParsedPlace::no_result()
            });
            PlaceLookupRow {
                query: query.to_string(),
                place_id: parsed.place_id,
                display_name: parsed.display_name,
                primary_type: parsed.primary_type,
                types: parsed.types,
                response_json: body,
                status: parsed.status,
            }
        }
        Err(e) => PlaceLookupRow {
            query: query.to_string(),
            place_id: None,
            display_name: None,
            primary_type: None,
            types: vec![],
            response_json: serde_json::json!({ "error": format!("{e:#}") }).to_string(),
            status: LookupStatus::Error,
        },
    };

    // Genuine outcomes (ok / no_result) are cached so a re-scan is free.
    // Transport/API errors are NOT cached, so a transient failure retries
    // on the next scan instead of sticking forever.
    if row.status != LookupStatus::Error {
        place_lookups::upsert(conn, &row)?;
    }
    Ok(row)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    const WOOLIES_BODY: &str = r#"{
        "places": [{
            "id": "ChIJWoolworths",
            "displayName": { "text": "Woolworths Strathfield", "languageCode": "en" },
            "primaryType": "supermarket",
            "types": ["supermarket", "grocery_store", "store", "point_of_interest"],
            "formattedAddress": "1 Plaza, Strathfield NSW"
        }]
    }"#;

    #[test]
    fn parses_first_place() {
        let p = parse_search_text(WOOLIES_BODY).unwrap();
        assert_eq!(p.place_id.as_deref(), Some("ChIJWoolworths"));
        assert_eq!(p.display_name.as_deref(), Some("Woolworths Strathfield"));
        assert_eq!(p.primary_type.as_deref(), Some("supermarket"));
        assert_eq!(p.types, vec!["supermarket", "grocery_store", "store", "point_of_interest"]);
        assert_eq!(p.status, LookupStatus::Ok);
    }

    #[test]
    fn empty_places_is_no_result() {
        let p = parse_search_text(r#"{"places":[]}"#).unwrap();
        assert_eq!(p.status, LookupStatus::NoResult);
        assert!(p.primary_type.is_none());
        assert!(p.types.is_empty());

        // Missing `places` key entirely also parses as no-result.
        let p2 = parse_search_text("{}").unwrap();
        assert_eq!(p2.status, LookupStatus::NoResult);
    }

    /// A fake client that records how many network calls it received, so
    /// we can prove the cache prevents a second call.
    struct FakeClient {
        body: String,
        calls: Cell<usize>,
    }
    impl PlacesClient for FakeClient {
        fn search_text(&self, _query: &str) -> Result<String> {
            self.calls.set(self.calls.get() + 1);
            Ok(self.body.clone())
        }
    }

    #[test]
    fn lookup_calls_network_on_miss_then_serves_from_cache() {
        let conn = crate::db::initialize_in_memory().unwrap();
        let client = FakeClient { body: WOOLIES_BODY.into(), calls: Cell::new(0) };

        // Miss -> one network call, result cached.
        let row = lookup(&conn, &client, "woolworths strathfield").unwrap();
        assert_eq!(row.primary_type.as_deref(), Some("supermarket"));
        assert_eq!(client.calls.get(), 1);

        // Hit -> no further network call.
        let row2 = lookup(&conn, &client, "woolworths strathfield").unwrap();
        assert_eq!(row2, row);
        assert_eq!(client.calls.get(), 1, "cache hit must not call the network");
    }

    /// A client that always errors, to prove a failure is cached (so we
    /// don't hammer a broken query) and surfaced as `Error`.
    struct ErrClient;
    impl PlacesClient for ErrClient {
        fn search_text(&self, _query: &str) -> Result<String> {
            anyhow::bail!("boom")
        }
    }

    #[test]
    fn lookup_does_not_cache_errors_so_they_retry() {
        let conn = crate::db::initialize_in_memory().unwrap();
        let row = lookup(&conn, &ErrClient, "broken").unwrap();
        assert_eq!(row.status, LookupStatus::Error);
        // The error row is NOT persisted (so a later scan retries it).
        assert!(place_lookups::get(&conn, "broken").unwrap().is_none());
    }
}

/// Shared in-crate test doubles for the Places client, usable by other
/// modules' tests (e.g. `scan`, `apply`). Compiled only under `test`.
#[cfg(test)]
pub mod tests_support {
    use super::*;
    use std::cell::Cell;

    /// A fake [`PlacesClient`] returning a fixed body and counting calls.
    pub struct FakeClient {
        body: String,
        calls: Cell<usize>,
    }

    impl FakeClient {
        /// Returns a Woolworths-style supermarket place.
        pub fn supermarket() -> Self {
            FakeClient {
                body: r#"{"places":[{"id":"ChIJ","displayName":{"text":"Woolworths"},"primaryType":"supermarket","types":["supermarket","grocery_store","store"]}]}"#.into(),
                calls: Cell::new(0),
            }
        }

        /// Returns a cafe place (maps to Eating Out).
        pub fn cafe() -> Self {
            FakeClient {
                body: r#"{"places":[{"id":"ChIJ","displayName":{"text":"Cafe"},"primaryType":"cafe","types":["cafe","food"]}]}"#.into(),
                calls: Cell::new(0),
            }
        }

        /// Returns a place whose type is not in the taxonomy.
        pub fn unmapped() -> Self {
            FakeClient {
                body: r#"{"places":[{"id":"ChIJ","displayName":{"text":"Zoo"},"primaryType":"zoo","types":["zoo"]}]}"#.into(),
                calls: Cell::new(0),
            }
        }

        pub fn calls(&self) -> usize {
            self.calls.get()
        }
    }

    impl PlacesClient for FakeClient {
        fn search_text(&self, _query: &str) -> Result<String> {
            self.calls.set(self.calls.get() + 1);
            Ok(self.body.clone())
        }
    }
}

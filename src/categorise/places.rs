use anyhow::{Context, Result};
use serde::Deserialize;

use super::cache::PlaceResult;

pub struct PlacesClient {
    api_key: String,
    http: ureq::Agent,
}

#[derive(Deserialize)]
struct SearchResponse {
    #[serde(default)]
    places: Vec<Place>,
}

#[derive(Deserialize)]
struct Place {
    #[serde(default)]
    types: Vec<String>,
    #[serde(rename = "displayName")]
    display_name: Option<DisplayName>,
    #[serde(rename = "formattedAddress")]
    formatted_address: Option<String>,
}

#[derive(Deserialize)]
struct DisplayName {
    text: Option<String>,
}

impl PlacesClient {
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            http: ureq::Agent::new_with_config(
                ureq::config::Config::builder()
                    .http_status_as_error(false)
                    .build(),
            ),
        }
    }

    pub fn search(&self, query: &str) -> Result<Option<PlaceResult>> {
        self.search_raw(query).map(|(result, _)| result)
    }

    pub fn search_raw(&self, query: &str) -> Result<(Option<PlaceResult>, String)> {
        let body = serde_json::json!({
            "textQuery": query,
            "maxResultCount": 1
        });

        let mut resp = self
            .http
            .post("https://places.googleapis.com/v1/places:searchText")
            .header("X-Goog-Api-Key", &self.api_key)
            .header(
                "X-Goog-FieldMask",
                "places.types,places.displayName,places.formattedAddress",
            )
            .header("Content-Type", "application/json")
            .send_json(&body)
            .context("Google Places API request failed")?;

        let status = resp.status();
        let body_str = resp.body_mut().read_to_string()?;

        if !status.is_success() {
            anyhow::bail!("Google Places API returned {}: {}", status, body_str);
        }

        let parsed: SearchResponse =
            serde_json::from_str(&body_str).context("Failed to parse Google Places response")?;

        let result = parsed.places.into_iter().next().map(|p| PlaceResult {
            place_name: p.display_name.and_then(|d| d.text),
            place_types: p.types,
            place_address: p.formatted_address,
        });

        Ok((result, body_str))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_places_response() {
        let json = r#"{
            "places": [{
                "types": ["supermarket", "grocery_store", "store"],
                "displayName": {"text": "Woolworths Strathfield", "languageCode": "en"},
                "formattedAddress": "123 The Boulevarde, Strathfield NSW 2135"
            }]
        }"#;
        let parsed: SearchResponse = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.places.len(), 1);
        assert_eq!(parsed.places[0].types, vec!["supermarket", "grocery_store", "store"]);
        assert_eq!(
            parsed.places[0].display_name.as_ref().unwrap().text,
            Some("Woolworths Strathfield".into())
        );
    }

    #[test]
    fn test_parse_empty_response() {
        let json = r#"{"places": []}"#;
        let parsed: SearchResponse = serde_json::from_str(json).unwrap();
        assert!(parsed.places.is_empty());
    }

    #[test]
    fn test_parse_no_places_key() {
        let json = r#"{}"#;
        let parsed: SearchResponse = serde_json::from_str(json).unwrap();
        assert!(parsed.places.is_empty());
    }

    #[test]
    #[ignore]
    fn test_real_api_call() {
        dotenvy::dotenv().ok();
        let api_key = std::env::var("GOOGLE_PLACES_API_KEY").unwrap();
        let client = PlacesClient::new(api_key);
        let result = client.search("Woolworths Strathfield").unwrap();
        assert!(result.is_some());
        let place = result.unwrap();
        assert!(place.place_types.iter().any(|t| t == "supermarket" || t == "grocery_store"));
    }
}

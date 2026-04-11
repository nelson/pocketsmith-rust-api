use anyhow::{Context, Result};
use serde::Deserialize;

use super::TARGET_CATEGORIES;

pub struct LlmClient {
    api_key: String,
    http: ureq::Agent,
}

#[derive(Deserialize)]
struct MessagesResponse {
    content: Vec<ContentBlock>,
}

#[derive(Deserialize)]
struct ContentBlock {
    text: Option<String>,
}

pub struct LlmCategorisation {
    pub payee: String,
    pub category: Option<String>,
    pub reason: String,
}

impl LlmClient {
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

    pub fn categorise_batch(&self, payees: &[String]) -> Result<Vec<LlmCategorisation>> {
        if payees.is_empty() {
            return Ok(Vec::new());
        }

        let categories_list = TARGET_CATEGORIES.join(", ");
        let payees_list = payees
            .iter()
            .enumerate()
            .map(|(i, p)| format!("{}. {}", i + 1, p))
            .collect::<Vec<_>>()
            .join("\n");

        let prompt = format!(
            "Categorise each merchant into exactly one of these categories:\n{}\n\n\
             Merchants:\n{}\n\n\
             For each merchant, respond with one line in this exact format:\n\
             <number>|<category>|<brief reason>\n\n\
             If you cannot determine a category, use: <number>|UNKNOWN|<reason>\n\
             Do not include any other text.",
            categories_list, payees_list
        );

        let body = serde_json::json!({
            "model": "claude-haiku-4-5-20251001",
            "max_tokens": 1024,
            "messages": [{"role": "user", "content": prompt}]
        });

        let mut resp = self
            .http
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .send_json(&body)
            .context("Anthropic API request failed")?;

        let status = resp.status();
        let body_str = resp.body_mut().read_to_string()?;

        if !status.is_success() {
            anyhow::bail!("Anthropic API returned {}: {}", status, body_str);
        }

        let parsed: MessagesResponse =
            serde_json::from_str(&body_str).context("Failed to parse Anthropic response")?;

        let text = parsed
            .content
            .into_iter()
            .filter_map(|b| b.text)
            .collect::<Vec<_>>()
            .join("");

        parse_batch_response(&text, payees)
    }
}

fn parse_batch_response(text: &str, payees: &[String]) -> Result<Vec<LlmCategorisation>> {
    let mut results: Vec<LlmCategorisation> = payees
        .iter()
        .map(|p| LlmCategorisation {
            payee: p.clone(),
            category: None,
            reason: "LLM: no response".into(),
        })
        .collect();

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.splitn(3, '|').collect();
        if parts.len() < 2 {
            continue;
        }
        let idx: usize = match parts[0].trim().parse::<usize>() {
            Ok(n) if n >= 1 && n <= payees.len() => n - 1,
            _ => continue,
        };
        let category = parts[1].trim().to_string();
        let reason = parts.get(2).map(|s| s.trim()).unwrap_or("").to_string();

        if category == "UNKNOWN" {
            results[idx].category = None;
            results[idx].reason = format!("LLM: unknown - {}", reason);
        } else if TARGET_CATEGORIES.contains(&category.as_str()) {
            results[idx].category = Some(category.clone());
            results[idx].reason = format!("LLM: {}", reason);
        } else {
            results[idx].reason = format!("LLM: invalid category '{}' - {}", category, reason);
        }
    }

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_batch_response() {
        let payees = vec![
            "Woolworths".into(),
            "Hero Sushi".into(),
            "Random Thing".into(),
        ];
        let text = "1|_Groceries|supermarket chain\n2|_Dining|sushi restaurant\n3|UNKNOWN|cannot determine";
        let results = parse_batch_response(text, &payees).unwrap();

        assert_eq!(results[0].category, Some("_Groceries".into()));
        assert_eq!(results[1].category, Some("_Dining".into()));
        assert!(results[2].category.is_none());
    }

    #[test]
    fn test_parse_partial_response() {
        let payees = vec!["A".into(), "B".into(), "C".into()];
        let text = "1|_Bills|utility\n3|_Shopping|retail";
        let results = parse_batch_response(text, &payees).unwrap();

        assert_eq!(results[0].category, Some("_Bills".into()));
        assert!(results[1].category.is_none()); // no response for B
        assert_eq!(results[2].category, Some("_Shopping".into()));
    }

    #[test]
    fn test_parse_invalid_category_ignored() {
        let payees = vec!["A".into()];
        let text = "1|_NotACategory|reason";
        let results = parse_batch_response(text, &payees).unwrap();
        assert!(results[0].category.is_none());
        assert!(results[0].reason.contains("invalid category"));
    }

    #[test]
    fn test_parse_empty_input() {
        let results = parse_batch_response("", &[]).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    #[ignore]
    fn test_real_llm_call() {
        dotenvy::dotenv().ok();
        let api_key = std::env::var("ANTHROPIC_API_KEY").unwrap();
        let client = LlmClient::new(api_key);
        let payees = vec!["Woolworths".into(), "Uber".into()];
        let results = client.categorise_batch(&payees).unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].category, Some("_Groceries".into()));
    }
}

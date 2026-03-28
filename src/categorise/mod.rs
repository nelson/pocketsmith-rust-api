use std::fmt;

pub mod rules;
pub mod cache;
pub mod places;
pub mod llm;
pub mod mapping;
pub mod pipeline;
pub mod eval;

#[derive(Debug, Clone, PartialEq)]
pub enum CategoriseSource {
    Rule,
    Cache,
    GooglePlaces,
    Llm,
    Unknown,
}

impl fmt::Display for CategoriseSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Rule => write!(f, "Rule"),
            Self::Cache => write!(f, "Cache"),
            Self::GooglePlaces => write!(f, "GooglePlaces"),
            Self::Llm => write!(f, "LLM"),
            Self::Unknown => write!(f, "Unknown"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CategoriseResult {
    pub normalised_payee: String,
    pub category: Option<String>,
    pub source: CategoriseSource,
    pub reason: String,
    pub confidence: f64,
    pub transaction_count: usize,
}

pub const TARGET_CATEGORIES: &[&str] = &[
    "_Bills", "_Dining", "_Education", "_Giving", "_Groceries",
    "_Holidays", "_Household", "_Income", "_Mortgage", "_Shopping",
    "_Transfer", "_Transport",
];

pub mod confidence {
    pub const TYPE_HIGH: f64 = 0.99;
    pub const PAYEE_OVERRIDE: f64 = 0.95;
    pub const PLACES_SPECIFIC: f64 = 0.90;
    pub const TYPE_BANKING: f64 = 0.80;
    pub const TYPE_DEFAULT: f64 = 0.70;
    pub const PLACES_GENERIC: f64 = 0.70;
    pub const LLM: f64 = 0.70;
}

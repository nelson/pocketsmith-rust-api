use std::fmt;

pub mod rules;
pub mod audit;
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
    pub transaction_count: usize,
}

pub const TARGET_CATEGORIES: &[&str] = &[
    "_Bills", "_Dining", "_Education", "_Giving", "_Groceries",
    "_Holidays", "_Household", "_Income", "_Mortgage", "_Shopping",
    "_Transfer", "_Transport",
];

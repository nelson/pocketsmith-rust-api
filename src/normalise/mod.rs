use serde_json::Value;
use std::collections::HashMap;

pub type Metadata = HashMap<String, Value>;

pub mod meta {
    pub const TYPE: &str = "type";
    pub const EXTRACTED_ENTITY: &str = "extracted_entity";
    pub const EXTRACT_KIND: &str = "extract_kind";
    pub const PREFIX_STRIPPED: &str = "prefix_stripped";
    pub const SUFFIXES_STRIPPED: &str = "suffixes_stripped";
}

pub mod rules;
pub mod strip;
pub mod classify;
pub mod expand;
pub mod identify;
pub mod cleanup;
pub mod main;

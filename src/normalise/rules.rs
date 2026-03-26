use anyhow::Result;
use serde::Deserialize;
use std::path::Path;

/// Regex wrapper: uses fast `regex` crate when possible, falls back to
/// `fancy_regex` only for patterns with lookahead/lookbehind.
pub enum Re {
    Fast(regex::Regex),
    Fancy(fancy_regex::Regex),
}

impl Re {
    pub fn new(pattern: &str) -> std::result::Result<Self, String> {
        match regex::Regex::new(pattern) {
            Ok(re) => Ok(Re::Fast(re)),
            Err(_) => fancy_regex::Regex::new(pattern)
                .map(Re::Fancy)
                .map_err(|e| e.to_string()),
        }
    }

    pub fn is_match(&self, text: &str) -> bool {
        match self {
            Re::Fast(re) => re.is_match(text),
            Re::Fancy(re) => re.is_match(text).unwrap_or(false),
        }
    }

    pub fn find(&self, text: &str) -> Option<(usize, usize)> {
        match self {
            Re::Fast(re) => re.find(text).map(|m| (m.start(), m.end())),
            Re::Fancy(re) => re.find(text).ok().flatten().map(|m| (m.start(), m.end())),
        }
    }

    pub fn captures<'t>(&self, text: &'t str) -> Option<ReCaptures<'t>> {
        match self {
            Re::Fast(re) => re.captures(text).map(ReCaptures::Fast),
            Re::Fancy(re) => re.captures(text).ok().flatten().map(ReCaptures::Fancy),
        }
    }

    pub fn replace_all<'t>(&self, text: &'t str, rep: &str) -> std::borrow::Cow<'t, str> {
        match self {
            Re::Fast(re) => re.replace_all(text, rep),
            Re::Fancy(re) => fancy_replace_all(re, text, rep),
        }
    }

    pub fn replace<'t>(&self, text: &'t str, rep: &str) -> std::borrow::Cow<'t, str> {
        match self {
            Re::Fast(re) => re.replace(text, rep),
            Re::Fancy(re) => match re.find(text) {
                Ok(Some(m)) => {
                    let mut result = String::new();
                    result.push_str(&text[..m.start()]);
                    result.push_str(rep);
                    result.push_str(&text[m.end()..]);
                    std::borrow::Cow::Owned(result)
                }
                _ => std::borrow::Cow::Borrowed(text),
            },
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Re::Fast(re) => re.as_str(),
            Re::Fancy(re) => re.as_str(),
        }
    }
}

fn fancy_replace_all<'t>(
    re: &fancy_regex::Regex,
    text: &'t str,
    rep: &str,
) -> std::borrow::Cow<'t, str> {
    let mut result = String::new();
    let mut last_end = 0;
    let mut search_start = 0;
    loop {
        match re.find(&text[search_start..]) {
            Ok(Some(m)) => {
                let abs_start = search_start + m.start();
                let abs_end = search_start + m.end();
                result.push_str(&text[last_end..abs_start]);
                result.push_str(rep);
                last_end = abs_end;
                if abs_end == search_start {
                    // Advance by one UTF-8 character to avoid infinite loop
                    let next = search_start
                        + text[search_start..].chars().next().map_or(1, |c| c.len_utf8());
                    if search_start < text.len() {
                        result.push_str(&text[search_start..next]);
                        last_end = next;
                    }
                    search_start = next;
                } else {
                    search_start = abs_end;
                }
            }
            _ => break,
        }
    }
    if last_end == 0 {
        return std::borrow::Cow::Borrowed(text);
    }
    result.push_str(&text[last_end..]);
    std::borrow::Cow::Owned(result)
}

pub enum ReCaptures<'t> {
    Fast(regex::Captures<'t>),
    Fancy(fancy_regex::Captures<'t>),
}

impl<'t> ReCaptures<'t> {
    pub fn get(&self, i: usize) -> Option<ReMatch<'t>> {
        match self {
            ReCaptures::Fast(caps) => caps.get(i).map(|m| ReMatch(m.as_str())),
            ReCaptures::Fancy(caps) => caps.get(i).map(|m| ReMatch(m.as_str())),
        }
    }
}

pub struct ReMatch<'t>(&'t str);

impl<'t> ReMatch<'t> {
    pub fn as_str(&self) -> &'t str {
        self.0
    }
}

pub fn escape(s: &str) -> String {
    regex::escape(s)
}

fn compile_re(pattern: &str, context: &str) -> Result<Re> {
    Re::new(pattern).map_err(|e| anyhow::anyhow!("Bad {} pattern '{}': {}", context, pattern, e))
}

fn compile_icase(pattern: &str, context: &str) -> Result<Re> {
    compile_re(&format!("(?i){}", pattern), context)
}

fn load_yaml<T: serde::de::DeserializeOwned>(rules_dir: &Path, filename: &str) -> Result<T> {
    let path = rules_dir.join(filename);
    let content = std::fs::read_to_string(&path)
        .map_err(|e| anyhow::anyhow!("Reading {}: {}", path.display(), e))?;
    serde_yaml::from_str(&content)
        .map_err(|e| anyhow::anyhow!("Parsing {}: {}", path.display(), e))
}

// ── Stage 1: Strip ──────────────────────────────────────────────────

#[derive(Deserialize)]
struct StripRulesYaml {
    #[serde(default)]
    prefixes: Vec<StripRuleYaml>,
    #[serde(default)]
    suffixes: Vec<StripRuleYaml>,
}

#[derive(Deserialize)]
struct StripRuleYaml {
    pattern: String,
    name: String,
    set_flag: Option<String>,
}

pub(super) struct CompiledStripRule {
    pub(super) re: Re,
    pub(super) name: String,
    pub(super) set_flag: Option<String>,
}

pub struct CompiledStripRules {
    pub(super) prefixes: Vec<CompiledStripRule>,
    pub(super) suffixes: Vec<CompiledStripRule>,
}

impl CompiledStripRules {
    pub fn load(rules_dir: &Path) -> Result<Self> {
        let yaml: StripRulesYaml = load_yaml(rules_dir, "strip.yaml")?;
        let compile = |r: StripRuleYaml| -> Result<CompiledStripRule> {
            Ok(CompiledStripRule {
                re: compile_icase(&r.pattern, "strip")?,
                name: r.name,
                set_flag: r.set_flag,
            })
        };
        Ok(Self {
            prefixes: yaml.prefixes.into_iter().map(compile).collect::<Result<_>>()?,
            suffixes: yaml.suffixes.into_iter().map(compile).collect::<Result<_>>()?,
        })
    }
}

// ── Stage 2: Classify ───────────────────────────────────────────────

#[derive(Deserialize)]
struct ClassifyRulesYaml {
    #[serde(default)]
    classification_rules: Vec<ClassifyRuleYaml>,
}

#[derive(Deserialize)]
struct ClassifyRuleYaml {
    pattern: String,
    #[serde(rename = "type")]
    payee_type: String,
    extract: Option<String>,
    extract_pattern: Option<String>,
}

pub(super) struct Extraction {
    pub(super) kind: String,
    pub(super) re: Re,
}

pub(super) struct CompiledClassifyRule {
    pub(super) re: Re,
    pub(super) payee_type: String,
    pub(super) extraction: Option<Extraction>,
}

pub struct CompiledClassifyRules {
    pub(super) rules: Vec<CompiledClassifyRule>,
}

impl CompiledClassifyRules {
    pub fn load(rules_dir: &Path) -> Result<Self> {
        let yaml: ClassifyRulesYaml = load_yaml(rules_dir, "classify.yaml")?;
        let rules = yaml
            .classification_rules
            .into_iter()
            .map(|r| {
                let extraction = match (r.extract, r.extract_pattern) {
                    (Some(kind), Some(pat)) => Some(Extraction {
                        kind,
                        re: compile_re(&pat, "classify extract")?,
                    }),
                    _ => None,
                };
                Ok(CompiledClassifyRule {
                    re: compile_re(&r.pattern, "classify")?,
                    payee_type: r.payee_type,
                    extraction,
                })
            })
            .collect::<Result<_>>()?;
        Ok(Self { rules })
    }
}

// ── Stage 3: Expand ─────────────────────────────────────────────────

#[derive(Deserialize)]
struct ExpandRulesYaml {
    #[serde(default)]
    suburbs: std::collections::HashMap<String, String>,
    #[serde(default)]
    words: std::collections::HashMap<String, String>,
    #[serde(default)]
    merchants: std::collections::HashMap<String, String>,
    #[serde(default)]
    known_locations: Vec<String>,
}

pub(super) struct ExpandPattern {
    pub(super) re: Re,
    pub(super) replacement: String,
    pub(super) truncated: String,
}

pub(super) struct LocationPattern {
    pub(super) re: Re,
    pub(super) name: String,
}

pub struct CompiledExpandRules {
    pub(super) patterns: Vec<ExpandPattern>,
    pub(super) locations: Vec<LocationPattern>,
}

impl CompiledExpandRules {
    pub fn load(rules_dir: &Path) -> Result<Self> {
        let yaml: ExpandRulesYaml = load_yaml(rules_dir, "expand.yaml")?;

        // Merge all dictionaries, uppercase keys
        let mut merged: Vec<(String, String)> = Vec::new();
        for map in [&yaml.suburbs, &yaml.words, &yaml.merchants] {
            for (k, v) in map {
                merged.push((k.to_uppercase(), v.to_uppercase()));
            }
        }
        // Sort by key length descending (longest match first)
        merged.sort_by(|a, b| b.0.len().cmp(&a.0.len()));

        // Compile word-boundary patterns, skipping no-ops
        let mut patterns = Vec::new();
        for (truncated, full) in &merged {
            if truncated != full {
                let pat = format!("(?i)\\b{}\\b", escape(truncated));
                patterns.push(ExpandPattern {
                    re: compile_re(&pat, "expand")?,
                    replacement: full.clone(),
                    truncated: truncated.clone(),
                });
            }
        }

        // Compile known_locations patterns (longest first)
        let mut locs: Vec<&String> = yaml.known_locations.iter().collect();
        locs.sort_by(|a, b| b.len().cmp(&a.len()));
        let locations = locs
            .into_iter()
            .map(|loc| {
                let pat = format!("(?i)\\b{}\\b", escape(&loc.to_uppercase()));
                Ok(LocationPattern {
                    re: compile_re(&pat, "expand location")?,
                    name: loc.clone(),
                })
            })
            .collect::<Result<_>>()?;

        Ok(Self { patterns, locations })
    }
}

// ── Stage 4: Identify ───────────────────────────────────────────────

#[derive(Deserialize)]
struct IdentifyRulesYaml {
    #[serde(default)]
    persons_strip_memo: Vec<String>,
    #[serde(default)]
    persons: std::collections::HashMap<String, String>,
    #[serde(default)]
    employers: Vec<EmployerYaml>,
    #[serde(default)]
    banking_identity_mappings: Vec<PatternCanonicalYaml>,
    #[serde(default)]
    merchant_mappings: Vec<PatternCanonicalYaml>,
    #[serde(default)]
    default_locations: std::collections::HashMap<String, String>,
    #[serde(default)]
    strip_patterns: Vec<StripPatternYaml>,
    #[serde(default)]
    internal_account_mappings: Vec<PatternCanonicalYaml>,
    #[serde(default)]
    transfer_entity_extraction: Vec<EntityExtractionYaml>,
    #[serde(default)]
    banking_entity_extraction: Vec<EntityExtractionYaml>,
    #[serde(default)]
    merchant_groups: Vec<MerchantGroupYaml>,
}

#[derive(Deserialize)]
struct EmployerYaml {
    patterns: Vec<String>,
    canonical: String,
}

#[derive(Deserialize)]
struct PatternCanonicalYaml {
    pattern: String,
    canonical: String,
}

#[derive(Deserialize)]
struct StripPatternYaml {
    pattern: String,
    #[allow(dead_code)]
    name: String,
}

#[derive(Deserialize)]
struct EntityExtractionYaml {
    pattern: String,
    group: Option<usize>,
    prefix: Option<String>,
}

#[derive(Deserialize)]
struct MerchantGroupYaml {
    pattern: String,
    group: String,
}

pub(super) struct CompiledPatternCanonical {
    pub(super) re: Re,
    pub(super) canonical: String,
}

pub(super) struct CompiledEntityExtraction {
    pub(super) re: Re,
    pub(super) group: usize,
    pub(super) prefix: Option<String>,
}

pub(super) struct CompiledMerchantGroup {
    pub(super) re: Re,
    pub(super) group: String,
}

pub(super) struct Employer {
    pub(super) patterns_upper: Vec<String>,
    pub(super) canonical: String,
}

pub struct CompiledIdentifyRules {
    pub(super) persons_strip_memo: Vec<String>,
    pub(super) persons: std::collections::HashMap<String, String>,
    pub(super) persons_upper: std::collections::HashMap<String, String>,
    pub(super) employers: Vec<Employer>,
    pub(super) banking_identity_mappings: Vec<CompiledPatternCanonical>,
    pub(super) merchant_mappings: Vec<CompiledPatternCanonical>,
    pub(super) default_locations: std::collections::HashMap<String, (String, String)>,
    pub(super) strip_patterns: Vec<Re>,
    pub(super) internal_account_mappings: Vec<CompiledPatternCanonical>,
    pub(super) transfer_entity_extraction: Vec<CompiledEntityExtraction>,
    pub(super) banking_entity_extraction: Vec<CompiledEntityExtraction>,
    pub(super) merchant_groups: Vec<CompiledMerchantGroup>,
    pub(super) capture_noise: Re,
    pub(super) title_re: Re,
}

impl CompiledIdentifyRules {
    pub fn load(rules_dir: &Path) -> Result<Self> {
        let yaml: IdentifyRulesYaml = load_yaml(rules_dir, "identify.yaml")?;

        let compile_pc = |items: Vec<PatternCanonicalYaml>, ctx: &str| -> Result<Vec<CompiledPatternCanonical>> {
            items.into_iter().map(|p| {
                Ok(CompiledPatternCanonical {
                    re: compile_icase(&p.pattern, ctx)?,
                    canonical: p.canonical,
                })
            }).collect()
        };

        let compile_ee = |items: Vec<EntityExtractionYaml>, ctx: &str| -> Result<Vec<CompiledEntityExtraction>> {
            items.into_iter().map(|e| {
                Ok(CompiledEntityExtraction {
                    re: compile_icase(&e.pattern, ctx)?,
                    group: e.group.unwrap_or(1),
                    prefix: e.prefix,
                })
            }).collect()
        };

        // Build case-insensitive persons lookup
        let mut persons_upper = std::collections::HashMap::new();
        for (k, v) in &yaml.persons {
            persons_upper.insert(k.to_uppercase(), v.clone());
        }

        // Build default_locations: key.upper() -> (key, location)
        let default_locations = yaml.default_locations.into_iter()
            .map(|(k, v)| (k.to_uppercase(), (k, v)))
            .collect();

        let capture_noise = compile_re(
            concat!(
                "(?i)(?:",
                "\\s+\\\\.*",
                "|\\s+\\S*\\d{2,}\\S*$",
                "|\\s+(?:NSW|Nsw|VIC|Vic|QLD|Qld|SA|WA|TAS|Tas|ACT|Act|NT)\\b.*$",
                "|\\s+(?:AU|AUS|Australia)\\b.*$",
                "|\\s*PTY\\.?\\s*LTD?\\.?",
                "|\\s+P/L(?=\\s|$)",
                "|\\s+PTY\\.?(?=\\s|$)",
                "|\\s*-\\s*$",
                ")"
            ),
            "capture noise",
        )?;

        let title_re = compile_icase("^(?:Mr|Mrs|Ms|Miss|Elder)\\s+", "title")?;

        Ok(Self {
            persons_strip_memo: yaml.persons_strip_memo,
            persons: yaml.persons,
            persons_upper,
            employers: yaml.employers.into_iter().map(|e| Employer {
                patterns_upper: e.patterns.into_iter().map(|p| p.to_uppercase()).collect(),
                canonical: e.canonical,
            }).collect(),
            banking_identity_mappings: compile_pc(yaml.banking_identity_mappings, "banking identity")?,
            merchant_mappings: compile_pc(yaml.merchant_mappings, "merchant mapping")?,
            default_locations,
            strip_patterns: yaml.strip_patterns.into_iter()
                .map(|s| compile_icase(&s.pattern, "strip pattern"))
                .collect::<Result<_>>()?,
            internal_account_mappings: compile_pc(yaml.internal_account_mappings, "internal account")?,
            transfer_entity_extraction: compile_ee(yaml.transfer_entity_extraction, "transfer entity")?,
            banking_entity_extraction: compile_ee(yaml.banking_entity_extraction, "banking entity")?,
            merchant_groups: yaml.merchant_groups.into_iter().map(|g| {
                Ok(CompiledMerchantGroup {
                    re: compile_icase(&g.pattern, "merchant group")?,
                    group: g.group,
                })
            }).collect::<Result<_>>()?,
            capture_noise,
            title_re,
        })
    }
}

//! Location-extraction stage (editable-rules-v3 PR 7).
//!
//! Scans the normalised payee for a known suburb/locality *anywhere in the
//! string* and records it in `features.location`. This is what the suffix
//! stage structurally can't do — suffix only matches the tail (and only the
//! trailing state/country code, which it records in `features.region`).
//!
//! The stage is **additive**: it does not strip the matched suburb from the
//! normalised string. Rules live in `rule_locations` and are read via the
//! [`RuleCache`](super::cache::RuleCache); there is no in-code const list.

use anyhow::Result;
use rusqlite::Connection;

use super::{NormalisationResult, PipelineCtx};

/// DB-backed location match. Sets `features.location` to the best known
/// suburb/city found anywhere in the normalised string, and
/// `features.region` to the best known region (country) found — the latter
/// only when the suffix stage didn't already capture a region code
/// (suffix's positional + postcode capture takes precedence).
pub fn apply_with_db(result: &mut NormalisationResult, ctx: &PipelineCtx) {
    match ctx.cache.locations(ctx.conn) {
        Ok(rules) => {
            // Record the match for the trace's `{pattern} ~= {string}`
            // line, preferring the location match; fall back to the region
            // match so a region-only hit still produces a match line
            // (consistent with the other matcher stages).
            let mut recorded = false;
            if let Some((loc, pattern, span)) = best_match(&result.normalised, &rules.locations) {
                result.record_match(pattern, span);
                result.features.location = Some(loc);
                recorded = true;
            }
            if result.features.region.is_none() {
                if let Some((reg, pattern, span)) = best_match(&result.normalised, &rules.regions) {
                    if !recorded {
                        result.record_match(pattern, span);
                    }
                    result.features.region = Some(reg);
                }
            }
        }
        Err(e) => eprintln!("locations: rule load failed, stage skipped: {e:#}"),
    }
}

/// Known place strings, partitioned by `kind` and longest-first within each
/// partition for deterministic matching.
pub(crate) struct LocationRules {
    pub locations: Vec<String>,
    pub regions: Vec<String>,
}

/// Load the known places (suburbs/cities as `location`, countries as
/// `region`), longest-first for deterministic matching. Rows come from
/// the typed [`crud::load_for_compile`] read; the `kind` is the typed
/// [`LocationKind`] from `rules::model` (rule-cli §3.1), replacing the
/// old stringly-typed match.
pub(crate) fn load_compiled(conn: &Connection) -> Result<LocationRules> {
    use crate::rules::{crud, model::{LocationKind, RuleData}, Stage};
    let mut rules = LocationRules { locations: Vec::new(), regions: Vec::new() };
    for data in crud::load_for_compile(conn, Stage::Locations)? {
        if let RuleData::Location { location, kind, .. } = data {
            match kind {
                LocationKind::Region => rules.regions.push(location),
                LocationKind::Location => rules.locations.push(location),
            }
        }
    }
    // Longest-first within each partition for deterministic matching
    // (crud returns id order, so sort here as the SQL ORDER BY used to).
    let by_len_desc =
        |a: &String, b: &String| b.len().cmp(&a.len()).then_with(|| a.cmp(b));
    rules.locations.sort_by(by_len_desc);
    rules.regions.sort_by(by_len_desc);
    Ok(rules)
}

/// Rightmost word-boundary index at which `needle` (already upper-case)
/// occurs in `hay` (already upper-case), or `None`. "Word boundary" means
/// the characters either side are not ASCII alphabetic — so `RYDE` matches
/// in `WEST RYDE` and `RYDE NSW` but not inside `RYDEAL`.
fn rightmost_word_boundary(hay: &str, needle: &str) -> Option<usize> {
    let hb = hay.as_bytes();
    let mut found = None;
    let mut from = 0;
    while let Some(rel) = hay[from..].find(needle) {
        let pos = from + rel;
        let end = pos + needle.len();
        let before_ok = pos == 0 || !hb[pos - 1].is_ascii_alphabetic();
        let after_ok = end == hb.len() || !hb[end].is_ascii_alphabetic();
        if before_ok && after_ok {
            found = Some(pos);
        }
        from = pos + 1;
    }
    found
}

/// Choose the best known location in `s`: the **longest** match, and among
/// equal-length matches the **rightmost** one (the locality nearest the
/// trailing region code is the true one, e.g. `ULTIMO` over `SYDNEY` in
/// "CAFE 10 SYDNEY ULTIMO"). Returns `(title-cased name, matched pattern,
/// span)`, where `span` is the byte range of the match in the *original*
/// `s`. The position comes from the upper-cased haystack, so it maps onto
/// `s` byte-for-byte only when `s` is ASCII (ASCII upper-casing is length-
/// and boundary-preserving); for any non-ASCII input (e.g. `ß` → `SS`
/// shifts boundaries) we return `None` for the span so the trace renders
/// the haystack un-highlighted rather than mis-sliced.
fn best_match(s: &str, locs: &[String]) -> Option<(String, String, Option<(usize, usize)>)> {
    let upper = s.to_uppercase();
    let mut best: Option<(usize, usize, &str)> = None; // (len, pos, loc)
    for loc in locs {
        if let Some(pos) = rightmost_word_boundary(&upper, loc) {
            let cand = (loc.len(), pos, loc.as_str());
            if best.map_or(true, |(bl, bp, _)| (cand.0, cand.1) > (bl, bp)) {
                best = Some(cand);
            }
        }
    }
    best.map(|(len, pos, loc)| {
        // `pos`/`len` index the upper-cased haystack; they map onto `s`
        // byte-for-byte only when `s` is ASCII. Drop the span otherwise.
        let span = s.is_ascii().then_some((pos, pos + len));
        (to_title_case(loc), loc.to_string(), span)
    })
}

fn to_title_case(s: &str) -> String {
    s.split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(c) => {
                    let upper: String = c.to_uppercase().collect();
                    upper + &chars.as_str().to_lowercase()
                }
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::normalise::cache;

    fn ctx_with(locs: &[&str]) -> rusqlite::Connection {
        let conn = crate::db::initialize_in_memory().unwrap();
        for l in locs {
            conn.execute("INSERT INTO rule_locations (location) VALUES (?1)", [l])
                .unwrap();
        }
        conn
    }

    fn run(conn: &rusqlite::Connection, payee: &str) -> Option<String> {
        let cache = cache::RuleCache::new();
        let c = cache::PipelineCtx::new(conn, &cache);
        let mut r = NormalisationResult::new(payee);
        apply_with_db(&mut r, &c);
        r.features.location
    }

    #[test]
    fn matches_mid_string() {
        let conn = ctx_with(&["STRATHFIELD"]);
        // Suburb buried mid-string with trailing metadata — suffix can't,
        // this stage can.
        assert_eq!(
            run(&conn, "GREENWAY MEAT In STRATHFIELD Date 05 Jul"),
            Some("Strathfield".into())
        );
    }

    #[test]
    fn longest_then_rightmost_wins() {
        let conn = ctx_with(&["SYDNEY", "ULTIMO", "NORTH STRATHFIELD", "STRATHFIELD"]);
        // Longest wins: NORTH STRATHFIELD beats STRATHFIELD.
        assert_eq!(run(&conn, "SHOP NORTH STRATHFIELD"), Some("North Strathfield".into()));
        // Equal length: rightmost wins (ULTIMO after SYDNEY).
        let conn2 = ctx_with(&["SYDNEY", "ULTIMO"]);
        assert_eq!(run(&conn2, "CAFE 10 SYDNEY ULTIMO"), Some("Ultimo".into()));
    }

    #[test]
    fn word_boundary_respected() {
        let conn = ctx_with(&["RYDE"]);
        assert_eq!(run(&conn, "WEST RYDE NSW"), Some("Ryde".into()));
        assert_eq!(run(&conn, "RYDEAL STORE"), None); // embedded, not a boundary
    }

    #[test]
    fn no_match_and_unseeded_are_noops() {
        let conn = ctx_with(&["STRATHFIELD"]);
        assert_eq!(run(&conn, "PLAIN PAYEE"), None);
        let bare = crate::db::initialize_in_memory().unwrap();
        assert_eq!(run(&bare, "GREENWAY STRATHFIELD"), None);
    }

    #[test]
    fn region_kind_sets_region_not_location() {
        let conn = crate::db::initialize_in_memory().unwrap();
        conn.execute(
            "INSERT INTO rule_locations (location, kind) VALUES ('SINGAPORE', 'region')",
            [],
        )
        .unwrap();
        let cache = cache::RuleCache::new();
        let c = cache::PipelineCtx::new(&conn, &cache);
        let mut r = NormalisationResult::new("MERCHANT Singapore");
        apply_with_db(&mut r, &c);
        assert_eq!(r.features.region.as_deref(), Some("Singapore"));
        assert_eq!(r.features.location, None, "region kind must not set location");
    }

    #[test]
    fn region_only_match_still_records_match_info() {
        // A region-only hit (no location match) must record match_info so
        // the trace shows the `{pattern} ~= {string}` line, like every
        // other matcher stage — not just the bare region feature.
        let conn = crate::db::initialize_in_memory().unwrap();
        conn.execute(
            "INSERT INTO rule_locations (location, kind) VALUES ('SINGAPORE', 'region')",
            [],
        )
        .unwrap();
        let cache = cache::RuleCache::new();
        let c = cache::PipelineCtx::new(&conn, &cache);
        let mut r = NormalisationResult::new("MERCHANT Singapore");
        apply_with_db(&mut r, &c);
        assert_eq!(r.features.location, None, "no location matched");
        let mi = r
            .pending_match
            .as_ref()
            .expect("region-only match must record match_info for the trace line");
        assert_eq!(mi.pattern, "SINGAPORE");
    }

    #[test]
    fn non_ascii_haystack_drops_span_to_avoid_misaligned_highlight() {
        // Upper-cased offsets only map onto a non-ASCII string unreliably
        // (e.g. ß → SS), so the span must be dropped — the match is still
        // recorded, just without a highlight range.
        let conn = crate::db::initialize_in_memory().unwrap();
        conn.execute("INSERT INTO rule_locations (location) VALUES ('SYDNEY')", [])
            .unwrap();
        let cache = cache::RuleCache::new();
        let c = cache::PipelineCtx::new(&conn, &cache);
        let mut r = NormalisationResult::new("Caf\u{e9} SYDNEY"); // non-ASCII 'é'
        apply_with_db(&mut r, &c);
        assert_eq!(r.features.location.as_deref(), Some("Sydney"));
        let mi = r.pending_match.as_ref().expect("match recorded");
        assert_eq!(mi.pattern, "SYDNEY");
        assert!(mi.span.is_none(), "non-ASCII haystack drops the span (graceful no-highlight)");
    }

    #[test]
    fn suffix_region_takes_precedence_over_list_region() {
        let conn = crate::db::initialize_in_memory().unwrap();
        conn.execute(
            "INSERT INTO rule_locations (location, kind) VALUES ('AUSTRALIA', 'region')",
            [],
        )
        .unwrap();
        let cache = cache::RuleCache::new();
        let c = cache::PipelineCtx::new(&conn, &cache);
        let mut r = NormalisationResult::new("WISE Australia");
        r.features.region = Some("NSW".into()); // as if suffix already set it
        apply_with_db(&mut r, &c);
        assert_eq!(r.features.region.as_deref(), Some("NSW"), "suffix region must win");
    }
}

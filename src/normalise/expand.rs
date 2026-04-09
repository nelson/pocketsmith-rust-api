use regex::Regex;
use std::sync::OnceLock;

use super::NormalisationResult;

struct Expansion {
    pattern: &'static str,
    canonical: &'static str,
}

struct CompiledExpansion {
    regex: Regex,
    canonical: &'static str,
}

/// Expand truncated words and country codes using word-boundary matching.
pub fn apply(result: &mut NormalisationResult) {
    loop {
        let mut matched = false;
        for exp in compiled_expansions() {
            if let Some(m) = exp.regex.find(&result.normalised) {
                result.normalised = format!(
                    "{}{}{}",
                    &result.normalised[..m.start()],
                    exp.canonical,
                    &result.normalised[m.end()..]
                );
                matched = true;
                break;
            }
        }
        if !matched {
            break;
        }
    }
}

const EXPANSIONS: &[Expansion] = &[
    // --- Suburb expansions (alphabetical by canonical, longer patterns first) ---
    Expansion { pattern: "ASHFIEL", canonical: "ASHFIELD" },
    Expansion { pattern: "BALGOWNI", canonical: "BALGOWNIE" },
    Expansion { pattern: "BARANGARO", canonical: "BARANGAROO" },
    Expansion { pattern: "BELFIEL", canonical: "BELFIELD" },
    Expansion { pattern: "BOMADERR", canonical: "BOMADERRY" },
    Expansion { pattern: "BROADWA", canonical: "BROADWAY" },
    Expansion { pattern: "BROADW", canonical: "BROADWAY" },
    Expansion { pattern: "BURWOO", canonical: "BURWOOD" },
    Expansion { pattern: "BURWO", canonical: "BURWOOD" },
    Expansion { pattern: "BURW", canonical: "BURWOOD" },
    Expansion { pattern: "CHARLESTOW", canonical: "CHARLESTOWN" },
    Expansion { pattern: "CHULLOR", canonical: "CHULLORA" },
    Expansion { pattern: "CONCOR", canonical: "CONCORD" },
    Expansion { pattern: "COOLANGATT", canonical: "COOLANGATTA" },
    Expansion { pattern: "CROWS NES", canonical: "CROWS NEST" },
    Expansion { pattern: "CROYD", canonical: "CROYDON" },
    Expansion { pattern: "DARLINGHURS", canonical: "DARLINGHURST" },
    Expansion { pattern: "DICKSO", canonical: "DICKSON" },
    Expansion { pattern: "FIVE DOC", canonical: "FIVE DOCK" },
    Expansion { pattern: "FORTITUD", canonical: "FORTITUDE VALLEY" },
    Expansion { pattern: "GATEWA", canonical: "GATEWAY" },
    Expansion { pattern: "HEBERFIELD", canonical: "HABERFIELD" },
    Expansion { pattern: "HABERFIEL", canonical: "HABERFIELD" },
    Expansion { pattern: "HEATHCO", canonical: "HEATHCOTE" },
    Expansion { pattern: "HOMEBUSH WES", canonical: "HOMEBUSH WEST" },
    Expansion { pattern: "HOMEBUSH WEA", canonical: "HOMEBUSH WEST" },
    Expansion { pattern: "HURSTV", canonical: "HURSTVILLE" },
    Expansion { pattern: "KIRRIBILL", canonical: "KIRRIBILLI" },
    Expansion { pattern: "MACQUARI", canonical: "MACQUARIE" },
    Expansion { pattern: "MACQUAR", canonical: "MACQUARIE" },
    Expansion { pattern: "MACQUARIE CEN", canonical: "MACQUARIE CENTRE" },
    Expansion { pattern: "MACQUARIE PAR", canonical: "MACQUARIE PARK" },
    Expansion { pattern: "MACQUARIE PA", canonical: "MACQUARIE PARK" },
    Expansion { pattern: "MARSFIEL", canonical: "MARSFIELD" },
    Expansion { pattern: "MARSFIE", canonical: "MARSFIELD" },
    Expansion { pattern: "MELBOURNE AIRPO", canonical: "MELBOURNE AIRPORT" },
    Expansion { pattern: "MONA VAL", canonical: "MONA VALE" },
    Expansion { pattern: "NEWINGT", canonical: "NEWINGTON" },
    Expansion { pattern: "NORTH RY", canonical: "NORTH RYDE" },
    Expansion { pattern: "NORTH STRATHFIE", canonical: "NORTH STRATHFIELD" },
    Expansion { pattern: "NORTH STRATHFAU", canonical: "NORTH STRATHFIELD" },
    Expansion { pattern: "NORTH STRATHF", canonical: "NORTH STRATHFIELD" },
    Expansion { pattern: "NORTH STRATH", canonical: "NORTH STRATHFIELD" },
    Expansion { pattern: "PALM BEAC", canonical: "PALM BEACH" },
    Expansion { pattern: "PARRAMATT", canonical: "PARRAMATTA" },
    Expansion { pattern: "PETERSHA", canonical: "PETERSHAM" },
    Expansion { pattern: "SHELL COV", canonical: "SHELL COVE" },
    Expansion { pattern: "SHELL C", canonical: "SHELL COVE" },
    Expansion { pattern: "SOUTH GRANVIL", canonical: "SOUTH GRANVILLE" },
    Expansion { pattern: "STANMOR", canonical: "STANMORE" },
    Expansion { pattern: "STRATHFIEL", canonical: "STRATHFIELD" },
    Expansion { pattern: "STRATHFIE", canonical: "STRATHFIELD" },
    Expansion { pattern: "STRATHFAU", canonical: "STRATHFIELD" },
    Expansion { pattern: "STRATHFI", canonical: "STRATHFIELD" },
    Expansion { pattern: "STRATHF", canonical: "STRATHFIELD" },
    Expansion { pattern: "STRATH", canonical: "STRATHFIELD" },
    Expansion { pattern: "STRAT", canonical: "STRATHFIELD" },
    Expansion { pattern: "SUMMER HIL", canonical: "SUMMER HILL" },
    Expansion { pattern: "SURFERS PARADIS", canonical: "SURFERS PARADISE" },
    Expansion { pattern: "WEST RY", canonical: "WEST RYDE" },
    Expansion { pattern: "WOLLONGON", canonical: "WOLLONGONG" },
    Expansion { pattern: "WOOLLOOMOOL", canonical: "WOOLLOOMOOLOO" },
    // --- Word truncations ---
    Expansion { pattern: "ACUPUNCT", canonical: "ACUPUNCTURE" },
    Expansion { pattern: "AUSTRA", canonical: "AUSTRALIA" },
    Expansion { pattern: "BOULEVAR", canonical: "BOULEVARD" },
    Expansion { pattern: "BREADTO", canonical: "BREADTOP" },
    Expansion { pattern: "CHEMIS", canonical: "CHEMIST" },
    Expansion { pattern: "CHEMI", canonical: "CHEMIST" },
    Expansion { pattern: "CHOCOLA", canonical: "CHOCOLATES" },
    Expansion { pattern: "COFF", canonical: "COFFEE" },
    Expansion { pattern: "COF", canonical: "COFFEE" },
    Expansion { pattern: "COSME", canonical: "COSMETICS" },
    Expansion { pattern: "ENTERTAINMEN", canonical: "ENTERTAINMENT" },
    Expansion { pattern: "ENTERTAIN", canonical: "ENTERTAINMENT" },
    Expansion { pattern: "ENTERPRISE", canonical: "ENTERPRISES" },
    Expansion { pattern: "ENTERPRIS", canonical: "ENTERPRISES" },
    Expansion { pattern: "ENTERPRI", canonical: "ENTERPRISES" },
    Expansion { pattern: "GELAT", canonical: "GELATO" },
    Expansion { pattern: "INTERNATIONA", canonical: "INTERNATIONAL" },
    Expansion { pattern: "INTERNATIO", canonical: "INTERNATIONAL" },
    Expansion { pattern: "KITCHE", canonical: "KITCHEN" },
    Expansion { pattern: "KITCH", canonical: "KITCHEN" },
    Expansion { pattern: "MKTPLC", canonical: "MARKETPLACE" },
    Expansion { pattern: "MKTPL", canonical: "MARKETPLACE" },
    Expansion { pattern: "PHARMCY", canonical: "PHARMACY" },
    Expansion { pattern: "RESTAURAN", canonical: "RESTAURANT" },
    Expansion { pattern: "RETA", canonical: "RETAIL" },
    Expansion { pattern: "STARBUC", canonical: "STARBUCKS" },
    Expansion { pattern: "SUPERMARKE", canonical: "SUPERMARKET" },
    Expansion { pattern: "SUPERMAR", canonical: "SUPERMARKET" },
    Expansion { pattern: "TOWE", canonical: "TOWER" },
    // --- Country-code expansions ---
    Expansion { pattern: "AUS", canonical: "Australia" },
    Expansion { pattern: "IDN", canonical: "Indonesia" },
    Expansion { pattern: "NLD", canonical: "Netherlands" },
    Expansion { pattern: "NSWAU", canonical: "NSW AU" },
    Expansion { pattern: "SGP", canonical: "Singapore" },
    Expansion { pattern: "GBR", canonical: "United Kingdom" },
    Expansion { pattern: "USA", canonical: "United States" },
];

fn compiled_expansions() -> &'static [CompiledExpansion] {
    static COMPILED: OnceLock<Vec<CompiledExpansion>> = OnceLock::new();
    COMPILED.get_or_init(|| {
        EXPANSIONS
            .iter()
            .map(|e| CompiledExpansion {
                regex: Regex::new(&format!("(?i)\\b{}\\b", regex::escape(e.pattern))).unwrap(),
                canonical: e.canonical,
            })
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::normalise::NormalisationResult;

    #[test]
    fn test_expand_nswau() {
        let mut r = NormalisationResult::new("MERCHANT NSWAU");
        apply(&mut r);
        assert_eq!(r.normalised, "MERCHANT NSW AU");
    }

    #[test]
    fn test_expand_nld() {
        let mut r = NormalisationResult::new("MERCHANT NLD");
        apply(&mut r);
        assert_eq!(r.normalised, "MERCHANT Netherlands");
    }

    #[test]
    fn test_expand_sgp() {
        let mut r = NormalisationResult::new("MERCHANT SGP");
        apply(&mut r);
        assert_eq!(r.normalised, "MERCHANT Singapore");
    }

    #[test]
    fn test_expand_usa() {
        let mut r = NormalisationResult::new("MERCHANT USA");
        apply(&mut r);
        assert_eq!(r.normalised, "MERCHANT United States");
    }

    #[test]
    fn test_expand_idn() {
        let mut r = NormalisationResult::new("MERCHANT IDN");
        apply(&mut r);
        assert_eq!(r.normalised, "MERCHANT Indonesia");
    }

    #[test]
    fn test_expand_gbr() {
        let mut r = NormalisationResult::new("MERCHANT GBR");
        apply(&mut r);
        assert_eq!(r.normalised, "MERCHANT United Kingdom");
    }

    #[test]
    fn test_expand_aus() {
        let mut r = NormalisationResult::new("MERCHANT AUS");
        apply(&mut r);
        assert_eq!(r.normalised, "MERCHANT Australia");
    }
}

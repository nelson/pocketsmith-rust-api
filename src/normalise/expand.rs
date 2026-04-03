use regex::Regex;
use std::sync::OnceLock;

use super::NormalisationResult;

struct Expansion {
    regex: Regex,
    to: &'static str,
    is_location: bool,
}

fn exp(from: &str, to: &'static str, is_location: bool) -> Expansion {
    let regex = Regex::new(&format!("(?i)\\b{}\\b", regex::escape(from))).unwrap();
    Expansion { regex, to, is_location }
}

fn truncation_expansions() -> &'static Vec<Expansion> {
    static EXPANSIONS: OnceLock<Vec<Expansion>> = OnceLock::new();
    EXPANSIONS.get_or_init(|| {
        vec![
            // Multi-word suburbs (longest first)
            exp("NORTH STRATHFIE", "NORTH STRATHFIELD", true),
            exp("NORTH STRATHFAU", "NORTH STRATHFIELD", true),
            exp("NORTH STRATHF", "NORTH STRATHFIELD", true),
            exp("NORTH STRATH", "NORTH STRATHFIELD", true),
            exp("STRATHFIEL", "STRATHFIELD", true),
            exp("STRATHFIE", "STRATHFIELD", true),
            exp("STRATHFI", "STRATHFIELD", true),
            exp("STRATHFAU", "STRATHFIELD", true),
            exp("STRATHF", "STRATHFIELD", true),
            exp("STRATH", "STRATHFIELD", true),
            exp("STRAT", "STRATHFIELD", true),
            exp("NORTH RY", "NORTH RYDE", true),
            exp("WEST RY", "WEST RYDE", true),
            exp("BURWOO", "BURWOOD", true),
            exp("BURWO", "BURWOOD", true),
            exp("BURW", "BURWOOD", true),
            exp("MACQUARIE PAR", "MACQUARIE PARK", true),
            exp("MACQUARIE PA", "MACQUARIE PARK", true),
            exp("MACQUARIE CEN", "MACQUARIE CENTRE", true),
            exp("MACQUARI", "MACQUARIE", true),
            exp("MACQUAR", "MACQUARIE", true),
            exp("HABERFIEL", "HABERFIELD", true),
            exp("HEBERFIELD", "HABERFIELD", true),
            exp("HOMEBUSH WES", "HOMEBUSH WEST", true),
            exp("HOMEBUSH WEA", "HOMEBUSH WEST", true),
            exp("SOUTH GRANVIL", "SOUTH GRANVILLE", true),
            exp("DARLINGHURS", "DARLINGHURST", true),
            exp("WOOLLOOMOOL", "WOOLLOOMOOLOO", true),
            exp("BALGOWNI", "BALGOWNIE", true),
            exp("COOLANGATT", "COOLANGATTA", true),
            exp("PARRAMATT", "PARRAMATTA", true),
            exp("BARANGARO", "BARANGAROO", true),
            exp("PETERSHA", "PETERSHAM", true),
            exp("STANMOR", "STANMORE", true),
            exp("SURFERS PARADIS", "SURFERS PARADISE", true),
            exp("MELBOURNE AIRPO", "MELBOURNE AIRPORT", true),
            exp("MARSFIEL", "MARSFIELD", true),
            exp("MARSFIE", "MARSFIELD", true),
            exp("NEWINGT", "NEWINGTON", true),
            exp("CHULLOR", "CHULLORA", true),
            exp("CONCOR", "CONCORD", true),
            exp("CROYD", "CROYDON", true),
            exp("PALM BEAC", "PALM BEACH", true),
            exp("MONA VAL", "MONA VALE", true),
            exp("SUMMER HIL", "SUMMER HILL", true),
            exp("BROADWA", "BROADWAY", true),
            exp("BROADW", "BROADWAY", true),
            exp("GATEWA", "GATEWAY", true),
            exp("CHARLESTOW", "CHARLESTOWN", true),
            exp("HEATHCO", "HEATHCOTE", true),
            exp("KIRRIBILL", "KIRRIBILLI", true),
            exp("SHELL COV", "SHELL COVE", true),
            exp("SHELL C", "SHELL COVE", true),
            exp("BOMADERR", "BOMADERRY", true),
            exp("WOLLONGON", "WOLLONGONG", true),
            exp("HURSTV", "HURSTVILLE", true),
            exp("FIVE DOC", "FIVE DOCK", true),
            exp("ASHFIEL", "ASHFIELD", true),
            exp("BELFIEL", "BELFIELD", true),
            exp("CROWS NES", "CROWS NEST", true),
            exp("DICKSO", "DICKSON", true),
            exp("FORTITUD", "FORTITUDE VALLEY", true),
            // Word truncations
            exp("PHARMCY", "PHARMACY", false),
            exp("MKTPL", "MARKETPLACE", false),
            exp("MKTPLC", "MARKETPLACE", false),
            exp("RETA", "RETAIL", false),
            exp("AUSTRA", "AUSTRALIA", false),
            exp("SUPERMARKE", "SUPERMARKET", false),
            exp("SUPERMAR", "SUPERMARKET", false),
            exp("RESTAURAN", "RESTAURANT", false),
            exp("INTERNATIO", "INTERNATIONAL", false),
            exp("INTERNATIONA", "INTERNATIONAL", false),
            exp("ENTERPRI", "ENTERPRISES", false),
            exp("ENTERPRIS", "ENTERPRISES", false),
            exp("ENTERPRISE", "ENTERPRISES", false),
            exp("CHOCOLA", "CHOCOLATES", false),
            exp("ACUPUNCT", "ACUPUNCTURE", false),
            exp("CHEMIS", "CHEMIST", false),
            exp("CHEMI", "CHEMIST", false),
            exp("KITCHE", "KITCHEN", false),
            exp("KITCH", "KITCHEN", false),
            exp("GELAT", "GELATO", false),
            exp("ENTERTAIN", "ENTERTAINMENT", false),
            exp("ENTERTAINMEN", "ENTERTAINMENT", false),
            exp("BOULEVAR", "BOULEVARD", false),
            exp("TOWE", "TOWER", false),
            exp("COF", "COFFEE", false),
            exp("COFF", "COFFEE", false),
            exp("COSME", "COSMETICS", false),
            exp("STARBUC", "STARBUCKS", false),
            exp("BREADTO", "BREADTOP", false),
        ]
    })
}

/// Expand truncated words in a payee string using word-boundary matching.
pub fn expand(result: &mut NormalisationResult) {
    let mut changed = true;

    while changed {
        changed = false;

        for exp in truncation_expansions() {
            if let Some(m) = exp.regex.find(&result.normalised) {
                result.normalised = format!("{}{}{}", &result.normalised[..m.start()], exp.to, &result.normalised[m.end()..]);
                if exp.is_location {
                    result.features.location = Some(exp.to.to_string());
                }
                changed = true;
                break;
            }
        }
    }
}

use regex::Regex;
use std::sync::OnceLock;

use super::locations;
use super::NormalisationResult;

struct Expansion {
    regex: Regex,
    to: &'static str,
}

fn exp(from: &str, to: &'static str) -> Expansion {
    let regex = Regex::new(&format!("(?i)\\b{}\\b", regex::escape(from))).unwrap();
    Expansion { regex, to }
}

fn truncation_expansions() -> &'static Vec<Expansion> {
    static EXPANSIONS: OnceLock<Vec<Expansion>> = OnceLock::new();
    EXPANSIONS.get_or_init(|| {
        vec![
            // Multi-word suburbs (longest first)
            exp("NORTH STRATHFIE", "NORTH STRATHFIELD"),
            exp("NORTH STRATHFAU", "NORTH STRATHFIELD"),
            exp("NORTH STRATHF", "NORTH STRATHFIELD"),
            exp("NORTH STRATH", "NORTH STRATHFIELD"),
            exp("STRATHFIEL", "STRATHFIELD"),
            exp("STRATHFIE", "STRATHFIELD"),
            exp("STRATHFI", "STRATHFIELD"),
            exp("STRATHFAU", "STRATHFIELD"),
            exp("STRATHF", "STRATHFIELD"),
            exp("STRATH", "STRATHFIELD"),
            exp("STRAT", "STRATHFIELD"),
            exp("NORTH RY", "NORTH RYDE"),
            exp("WEST RY", "WEST RYDE"),
            exp("BURWOO", "BURWOOD"),
            exp("BURWO", "BURWOOD"),
            exp("BURW", "BURWOOD"),
            exp("MACQUARIE PAR", "MACQUARIE PARK"),
            exp("MACQUARIE PA", "MACQUARIE PARK"),
            exp("MACQUARIE CEN", "MACQUARIE CENTRE"),
            exp("MACQUARI", "MACQUARIE"),
            exp("MACQUAR", "MACQUARIE"),
            exp("HABERFIEL", "HABERFIELD"),
            exp("HEBERFIELD", "HABERFIELD"),
            exp("HOMEBUSH WES", "HOMEBUSH WEST"),
            exp("HOMEBUSH WEA", "HOMEBUSH WEST"),
            exp("SOUTH GRANVIL", "SOUTH GRANVILLE"),
            exp("DARLINGHURS", "DARLINGHURST"),
            exp("WOOLLOOMOOL", "WOOLLOOMOOLOO"),
            exp("BALGOWNI", "BALGOWNIE"),
            exp("COOLANGATT", "COOLANGATTA"),
            exp("PARRAMATT", "PARRAMATTA"),
            exp("BARANGARO", "BARANGAROO"),
            exp("PETERSHA", "PETERSHAM"),
            exp("STANMOR", "STANMORE"),
            exp("SURFERS PARADIS", "SURFERS PARADISE"),
            exp("MELBOURNE AIRPO", "MELBOURNE AIRPORT"),
            exp("MARSFIEL", "MARSFIELD"),
            exp("MARSFIE", "MARSFIELD"),
            exp("NEWINGT", "NEWINGTON"),
            exp("CHULLOR", "CHULLORA"),
            exp("CONCOR", "CONCORD"),
            exp("CROYD", "CROYDON"),
            exp("PALM BEAC", "PALM BEACH"),
            exp("MONA VAL", "MONA VALE"),
            exp("SUMMER HIL", "SUMMER HILL"),
            exp("BROADWA", "BROADWAY"),
            exp("BROADW", "BROADWAY"),
            exp("GATEWA", "GATEWAY"),
            exp("CHARLESTOW", "CHARLESTOWN"),
            exp("HEATHCO", "HEATHCOTE"),
            exp("KIRRIBILL", "KIRRIBILLI"),
            exp("SHELL COV", "SHELL COVE"),
            exp("SHELL C", "SHELL COVE"),
            exp("BOMADERR", "BOMADERRY"),
            exp("WOLLONGON", "WOLLONGONG"),
            exp("HURSTV", "HURSTVILLE"),
            exp("FIVE DOC", "FIVE DOCK"),
            exp("ASHFIEL", "ASHFIELD"),
            exp("BELFIEL", "BELFIELD"),
            exp("CROWS NES", "CROWS NEST"),
            exp("DICKSO", "DICKSON"),
            exp("FORTITUD", "FORTITUDE VALLEY"),
            // Word truncations
            exp("PHARMCY", "PHARMACY"),
            exp("MKTPL", "MARKETPLACE"),
            exp("MKTPLC", "MARKETPLACE"),
            exp("RETA", "RETAIL"),
            exp("AUSTRA", "AUSTRALIA"),
            exp("SUPERMARKE", "SUPERMARKET"),
            exp("SUPERMAR", "SUPERMARKET"),
            exp("RESTAURAN", "RESTAURANT"),
            exp("INTERNATIO", "INTERNATIONAL"),
            exp("INTERNATIONA", "INTERNATIONAL"),
            exp("ENTERPRI", "ENTERPRISES"),
            exp("ENTERPRIS", "ENTERPRISES"),
            exp("ENTERPRISE", "ENTERPRISES"),
            exp("CHOCOLA", "CHOCOLATES"),
            exp("ACUPUNCT", "ACUPUNCTURE"),
            exp("CHEMIS", "CHEMIST"),
            exp("CHEMI", "CHEMIST"),
            exp("KITCHE", "KITCHEN"),
            exp("KITCH", "KITCHEN"),
            exp("GELAT", "GELATO"),
            exp("ENTERTAIN", "ENTERTAINMENT"),
            exp("ENTERTAINMEN", "ENTERTAINMENT"),
            exp("BOULEVAR", "BOULEVARD"),
            exp("TOWE", "TOWER"),
            exp("COF", "COFFEE"),
            exp("COFF", "COFFEE"),
            exp("COSME", "COSMETICS"),
            exp("STARBUC", "STARBUCKS"),
            exp("BREADTO", "BREADTOP"),
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
                if locations::is_known_location(exp.to) {
                    result.features.location = Some(exp.to.to_string());
                }
                changed = true;
                break;
            }
        }
    }
}

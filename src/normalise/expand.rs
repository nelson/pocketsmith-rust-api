use std::sync::OnceLock;

struct Expansion {
    from: &'static str,
    to: &'static str,
    is_location: bool,
}

fn truncation_expansions() -> &'static Vec<Expansion> {
    static EXPANSIONS: OnceLock<Vec<Expansion>> = OnceLock::new();
    EXPANSIONS.get_or_init(|| {
        vec![
            // Multi-word suburbs (longest first)
            Expansion { from: "NORTH STRATHFIE", to: "NORTH STRATHFIELD", is_location: true },
            Expansion { from: "NORTH STRATHFAU", to: "NORTH STRATHFIELD", is_location: true },
            Expansion { from: "NORTH STRATHF", to: "NORTH STRATHFIELD", is_location: true },
            Expansion { from: "NORTH STRATH", to: "NORTH STRATHFIELD", is_location: true },
            Expansion { from: "STRATHFIEL", to: "STRATHFIELD", is_location: true },
            Expansion { from: "STRATHFIE", to: "STRATHFIELD", is_location: true },
            Expansion { from: "STRATHFI", to: "STRATHFIELD", is_location: true },
            Expansion { from: "STRATHFAU", to: "STRATHFIELD", is_location: true },
            Expansion { from: "STRATHF", to: "STRATHFIELD", is_location: true },
            Expansion { from: "STRATH", to: "STRATHFIELD", is_location: true },
            Expansion { from: "STRAT", to: "STRATHFIELD", is_location: true },
            Expansion { from: "NORTH RY", to: "NORTH RYDE", is_location: true },
            Expansion { from: "WEST RY", to: "WEST RYDE", is_location: true },
            Expansion { from: "BURWOO", to: "BURWOOD", is_location: true },
            Expansion { from: "BURWO", to: "BURWOOD", is_location: true },
            Expansion { from: "BURW", to: "BURWOOD", is_location: true },
            Expansion { from: "MACQUARIE PAR", to: "MACQUARIE PARK", is_location: true },
            Expansion { from: "MACQUARIE PA", to: "MACQUARIE PARK", is_location: true },
            Expansion { from: "MACQUARIE CEN", to: "MACQUARIE CENTRE", is_location: true },
            Expansion { from: "MACQUARI", to: "MACQUARIE", is_location: true },
            Expansion { from: "MACQUAR", to: "MACQUARIE", is_location: true },
            Expansion { from: "HABERFIEL", to: "HABERFIELD", is_location: true },
            Expansion { from: "HEBERFIELD", to: "HABERFIELD", is_location: true },
            Expansion { from: "HOMEBUSH WES", to: "HOMEBUSH WEST", is_location: true },
            Expansion { from: "HOMEBUSH WEA", to: "HOMEBUSH WEST", is_location: true },
            Expansion { from: "SOUTH GRANVIL", to: "SOUTH GRANVILLE", is_location: true },
            Expansion { from: "DARLINGHURS", to: "DARLINGHURST", is_location: true },
            Expansion { from: "WOOLLOOMOOL", to: "WOOLLOOMOOLOO", is_location: true },
            Expansion { from: "BALGOWNI", to: "BALGOWNIE", is_location: true },
            Expansion { from: "COOLANGATT", to: "COOLANGATTA", is_location: true },
            Expansion { from: "PARRAMATT", to: "PARRAMATTA", is_location: true },
            Expansion { from: "BARANGARO", to: "BARANGAROO", is_location: true },
            Expansion { from: "PETERSHA", to: "PETERSHAM", is_location: true },
            Expansion { from: "STANMOR", to: "STANMORE", is_location: true },
            Expansion { from: "SURFERS PARADIS", to: "SURFERS PARADISE", is_location: true },
            Expansion { from: "MELBOURNE AIRPO", to: "MELBOURNE AIRPORT", is_location: true },
            Expansion { from: "MARSFIEL", to: "MARSFIELD", is_location: true },
            Expansion { from: "MARSFIE", to: "MARSFIELD", is_location: true },
            Expansion { from: "NEWINGT", to: "NEWINGTON", is_location: true },
            Expansion { from: "CHULLOR", to: "CHULLORA", is_location: true },
            Expansion { from: "CONCOR", to: "CONCORD", is_location: true },
            Expansion { from: "CROYD", to: "CROYDON", is_location: true },
            Expansion { from: "PALM BEAC", to: "PALM BEACH", is_location: true },
            Expansion { from: "MONA VAL", to: "MONA VALE", is_location: true },
            Expansion { from: "SUMMER HIL", to: "SUMMER HILL", is_location: true },
            Expansion { from: "BROADWA", to: "BROADWAY", is_location: true },
            Expansion { from: "BROADW", to: "BROADWAY", is_location: true },
            Expansion { from: "GATEWA", to: "GATEWAY", is_location: true },
            Expansion { from: "CHARLESTOW", to: "CHARLESTOWN", is_location: true },
            Expansion { from: "HEATHCO", to: "HEATHCOTE", is_location: true },
            Expansion { from: "KIRRIBILL", to: "KIRRIBILLI", is_location: true },
            Expansion { from: "SHELL COV", to: "SHELL COVE", is_location: true },
            Expansion { from: "SHELL C", to: "SHELL COVE", is_location: true },
            Expansion { from: "BOMADERR", to: "BOMADERRY", is_location: true },
            Expansion { from: "WOLLONGON", to: "WOLLONGONG", is_location: true },
            Expansion { from: "HURSTV", to: "HURSTVILLE", is_location: true },
            Expansion { from: "FIVE DOC", to: "FIVE DOCK", is_location: true },
            Expansion { from: "ASHFIEL", to: "ASHFIELD", is_location: true },
            Expansion { from: "BELFIEL", to: "BELFIELD", is_location: true },
            Expansion { from: "CROWS NES", to: "CROWS NEST", is_location: true },
            Expansion { from: "DICKSO", to: "DICKSON", is_location: true },
            Expansion { from: "FORTITUD", to: "FORTITUDE VALLEY", is_location: true },
            // Word truncations
            Expansion { from: "PHARMCY", to: "PHARMACY", is_location: false },
            Expansion { from: "MKTPL", to: "MARKETPLACE", is_location: false },
            Expansion { from: "MKTPLC", to: "MARKETPLACE", is_location: false },
            Expansion { from: "RETA", to: "RETAIL", is_location: false },
            Expansion { from: "AUSTRA", to: "AUSTRALIA", is_location: false },
            Expansion { from: "SUPERMARKE", to: "SUPERMARKET", is_location: false },
            Expansion { from: "SUPERMAR", to: "SUPERMARKET", is_location: false },
            Expansion { from: "RESTAURAN", to: "RESTAURANT", is_location: false },
            Expansion { from: "INTERNATIO", to: "INTERNATIONAL", is_location: false },
            Expansion { from: "INTERNATIONA", to: "INTERNATIONAL", is_location: false },
            Expansion { from: "ENTERPRI", to: "ENTERPRISES", is_location: false },
            Expansion { from: "ENTERPRIS", to: "ENTERPRISES", is_location: false },
            Expansion { from: "ENTERPRISE", to: "ENTERPRISES", is_location: false },
            Expansion { from: "CHOCOLA", to: "CHOCOLATES", is_location: false },
            Expansion { from: "ACUPUNCT", to: "ACUPUNCTURE", is_location: false },
            Expansion { from: "CHEMIS", to: "CHEMIST", is_location: false },
            Expansion { from: "CHEMI", to: "CHEMIST", is_location: false },
            Expansion { from: "KITCHE", to: "KITCHEN", is_location: false },
            Expansion { from: "KITCH", to: "KITCHEN", is_location: false },
            Expansion { from: "GELAT", to: "GELATO", is_location: false },
            Expansion { from: "ENTERTAIN", to: "ENTERTAINMENT", is_location: false },
            Expansion { from: "ENTERTAINMEN", to: "ENTERTAINMENT", is_location: false },
            Expansion { from: "BOULEVAR", to: "BOULEVARD", is_location: false },
            Expansion { from: "TOWE", to: "TOWER", is_location: false },
            Expansion { from: "COF", to: "COFFEE", is_location: false },
            Expansion { from: "COFF", to: "COFFEE", is_location: false },
            Expansion { from: "COSME", to: "COSMETICS", is_location: false },
            Expansion { from: "STARBUC", to: "STARBUCKS", is_location: false },
            Expansion { from: "BREADTO", to: "BREADTOP", is_location: false },
        ]
    })
}

/// Expand truncated words in a payee string using word-boundary matching.
pub fn expand_truncations(s: &str) -> String {
    let mut result = s.to_string();
    let mut changed = true;

    while changed {
        changed = false;
        let upper = result.to_uppercase();

        for exp in truncation_expansions() {
            if exp.from == exp.to {
                continue;
            }

            if let Some(pos) = upper.find(exp.from) {
                let at_word_start =
                    pos == 0 || !upper.as_bytes()[pos - 1].is_ascii_alphanumeric();
                let end = pos + exp.from.len();
                let at_word_end =
                    end == upper.len() || !upper.as_bytes()[end].is_ascii_alphanumeric();

                if at_word_start && at_word_end {
                    result = format!("{}{}{}", &result[..pos], exp.to, &result[end..]);
                    changed = true;
                    break;
                }
            }
        }
    }

    result
}

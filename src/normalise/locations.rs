const KNOWN_LOCATIONS: &[&str] = &[
    "NORTH STRATHFIELD", "STRATHFIELD SOUTH", "NORTH PARRAMATTA",
    "SOUTH GRANVILLE", "MACQUARIE CENTRE", "SURFERS PARADISE",
    "MELBOURNE AIRPORT", "FORTITUDE VALLEY", "SYDNEY AIRPORT",
    "MACQUARIE PARK", "HOMEBUSH WEST", "WEST MELBOURNE",
    "CROYDON PARK", "SUMMER HILL", "BAULKHAM HILLS",
    "EASTERN CREEK", "PENNANT HILLS", "MARTIN PLACE",
    "FAIRY MEADOW", "THE ENTRANCE", "NORTH RYDE", "WEST RYDE",
    "SHELL COVE", "MONA VALE", "PALM BEACH", "SURRY HILLS",
    "CROWS NEST", "FIVE DOCK", "STRATHFIELD", "BURWOOD",
    "BROADWAY", "SYDNEY", "MELBOURNE", "CHIPPENDALE", "ULTIMO",
    "BOWRAL", "TEMPE", "CROYDON", "ENFIELD", "NEWINGTON",
    "CONCORD", "RHODES", "HEATHCOTE", "BOMADERRY", "WOLLONGONG",
    "HURSTVILLE", "KINGSFORD", "MARSFIELD", "ASHFIELD", "BELFIELD",
    "DICKSON", "MASCOT", "AUBURN", "PADDINGTON", "DARLINGHURST",
    "KIRRIBILLI", "STANMORE", "PETERSHAM", "HABERFIELD", "CHULLORA",
    "SILVERWATER", "PARRAMATTA", "BARANGAROO", "WYNYARD",
    "SUNNYVALE", "SAN FRANCISCO", "CHARLESTOWN", "THE ROCKS",
    "HAYMARKET", "GATEWAY", "MACQUARIE", "COOLANGATTA",
    "WOOLLOOMOOLOO", "BALGOWNIE", "CHATSWOOD", "BLACKTOWN",
    "LIDCOMBE", "GREENACRE", "ENGADINE", "BLAXLAND", "GOULBURN",
    "KATOOMBA", "EPPING", "RYDE", "HOMEBUSH", "DURAL", "OURIMBAH",
    "BALMAIN", "BANKSTOWN", "MOOREBANK", "PYRMONT", "WESTMEAD",
    "NORTHMEAD", "LYNEHAM", "CAMPSIE",
];

/// Extract a known location from a payee string by word-boundary scan.
/// Returns the location in title case.
pub fn extract_location(s: &str) -> Option<String> {
    let upper = s.to_uppercase();
    // Try multi-word locations first (longer matches take priority)
    for &loc in KNOWN_LOCATIONS {
        if loc.contains(' ') && upper.contains(loc) {
            return Some(to_title_case(loc));
        }
    }
    // Then single-word locations with word boundary check
    for &loc in KNOWN_LOCATIONS {
        if !loc.contains(' ') {
            if let Some(pos) = upper.find(loc) {
                let before_ok = pos == 0 || !upper.as_bytes()[pos - 1].is_ascii_alphabetic();
                let end = pos + loc.len();
                let after_ok = end == upper.len() || !upper.as_bytes()[end].is_ascii_alphabetic();
                if before_ok && after_ok {
                    return Some(to_title_case(loc));
                }
            }
        }
    }
    None
}

/// Check if a string is a known location (case-insensitive).
pub fn is_known_location(s: &str) -> bool {
    let upper = s.to_uppercase();
    KNOWN_LOCATIONS.iter().any(|&loc| loc == upper)
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

    #[test]
    fn test_extract_location_strathfield() {
        let result = extract_location("WOOLWORTHS 1624 STRATHFIELD");
        assert_eq!(result, Some("Strathfield".to_string()));
    }

    #[test]
    fn test_is_known_location() {
        assert!(is_known_location("STRATHFIELD"));
        assert!(is_known_location("strathfield"));
        assert!(!is_known_location("UNKNOWN"));
    }
}

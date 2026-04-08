use std::sync::OnceLock;

use regex::Regex;

use super::{extract_features, NormalisationResult};

struct Prefix {
    pattern: &'static str,
    gateway: Option<&'static str>,
    has_date: bool,
}

struct CompiledPrefix {
    regex: Regex,
    gateway: Option<&'static str>,
    has_date: bool,
}

/// Strip metadata prefixes in a loop until no more match.
/// Returns true if any prefix was stripped (for callers that loop over both prefix+suffix).
pub fn strip_prefixes(result: &mut NormalisationResult) -> bool {
    let mut any_matched = false;
    loop {
        let mut matched = false;
        for pat in data() {
            if let Some(caps) = pat.regex.captures(&result.normalised) {
                extract_features(&caps, &mut result.features);
                if let Some(gw) = pat.gateway {
                    result.features.gateway = Some(gw.to_string());
                }
                result.normalised = result.normalised[caps.get(0).unwrap().end()..].to_string();
                matched = true;
                any_matched = true;
                break;
            }
        }
        if !matched {
            break;
        }
    }
    any_matched
}

// @cc use Prefix and CompiledPrefix
fn data() -> &'static [Prefix] {
    static PATTERNS: OnceLock<Vec<Prefix>> = OnceLock::new();
    PATTERNS.get_or_init(|| {
        let patterns: Vec<(&str, Option<&'static str>)> = vec![
            // --- Non-gateway prefixes ---
            (r"^(?P<date>\d{2}/\d{2}/\d{2,4}),?\s+", None),
            (r"^-([A-Z]+-)*", None),
            (r"^EFTPOS\s+", None),
            (r"^\*\s+", None),
            (r"^\s*-\s+", None),
            (r"^% ", None),
            (r"^Return\s+", None),
            (r"^SP ", None),
            (r"^Visa Debit Purchase Card (?P<account_ref>\d{4})\s+", None),
            // --- Gateway prefixes ---
            (r"^ALI\*", Some("AliExpress")),
            (r"^Alipay ", Some("Alipay")),
            (r"^CKO\*", Some("Checkout.com")),
            (r"^DBS\*", Some("DBS")),
            (r"^DNH\*", Some("DNH")),
            (r"^DOORDASH\*", Some("DoorDash")),
            (r"^EB\s*\*", Some("Eventbrite")),
            (r"^EZI\*", Some("Ezi")),
            (r"^FLEXISCHOOLS\*", Some("Flexischools")),
            (r"^GLOBAL-E\* ", Some("Global-E")),
            (r"^LIGHTSPEED\*(?:SR-)?(?:LS\s+)?", Some("Lightspeed")),
            (r"^LIME\*", Some("Lime")),
            (r"^LS\s+", Some("Lightspeed")),
            (r"^MPASS \*", Some("mPass")),
            (r"^MR YUM\*", Some("Mr Yum")),
            (r"^NAYAXAU\*", Some("Nayax")),
            (r"^PAYPAL \*", Some("PayPal")),
            (r"^PP\*", Some("PP")),
            (r"^(?i:Revolut)\*", Some("Revolut")),
            (r"^SMP\*", Some("Square Marketplace")),
            (r"^SQ \*", Some("Square")),
            (r"^TITHE\.LY\*", Some("Tithe.ly")),
            (r"^TST\*\s*", Some("Toast")),
            (r"^TRYBOOKING\*", Some("TryBooking")),
            (r"^Weixin ", Some("Weixin")),
            (r"^WINDCAVE\*", Some("Windcave")),
            (r"^ZLR\*", Some("Zeller")),
        ];
        patterns
            .into_iter()
            .map(|(p, gw)| Prefix {
                regex: Regex::new(p).expect("invalid prefix pattern"),
                gateway: gw,
            })
            .collect()
    })
}

use std::sync::OnceLock;

use regex::Regex;

use super::NormalisationResult;

// @cc most prefixes don't have accounts or dates. Can we use a default value?
struct Prefix {
    pattern: &'static str,
    gateway: Option<&'static str>,
    has_account: bool,
    has_date: bool,
}

struct CompiledPrefix {
    regex: Regex,
    gateway: Option<&'static str>,
    has_account: bool,
    has_date: bool,
}

/// Strip metadata prefixes in a loop until no more match.
pub fn apply(result: &mut NormalisationResult) {
    loop {
        let mut matched = false;
        for pat in data() {
            if let Some(caps) = pat.regex.captures(&result.normalised) {
                if let Some(gw) = pat.gateway {
                    result.features.gateway = Some(gw.to_string());
                }
                if pat.has_date {
                    if let Some(date) = caps.name("date") {
                        result.features.date = Some(date.as_str().to_string());
                    }
                }
                if pat.has_account {
                    if let Some(account) = caps.name("account") {
                        result.features.account = Some(account.as_str().to_string());
                    }
                }
                // @cc what does this line do, can it be simplified?
                result.normalised = result.normalised[caps.get(0).unwrap().end()..].trim().to_string();
                matched = true;
                break;
            }
        }
        if !matched {
            break;
        }
    }
}

const PREFIXES: &[Prefix] = &[
    // --- Non-gateway prefixes ---
    Prefix { pattern: r"^(?P<date>\d{2}/\d{2}/\d{2,4}),?\s+", gateway: None, has_account: false, has_date: true },
    Prefix { pattern: r"^-([A-Z]+-)*", gateway: None, has_account: false, has_date: false },
    Prefix { pattern: r"^EFTPOS\s+", gateway: None, has_account: false, has_date: false },
    Prefix { pattern: r"^\*\s+", gateway: None, has_account: false, has_date: false },
    Prefix { pattern: r"^\s*-\s+", gateway: None, has_account: false, has_date: false },
    Prefix { pattern: r"^% ", gateway: None, has_account: false, has_date: false },
    Prefix { pattern: r"^Return\s+", gateway: None, has_account: false, has_date: false },
    Prefix { pattern: r"^SP ", gateway: None, has_account: false, has_date: false },
    Prefix { pattern: r"^Visa Debit Purchase Card (?P<account>\d{4})\s+", gateway: None, has_account: true, has_date: false },
    // --- Gateway prefixes ---
    Prefix { pattern: r"^ALI\*", gateway: Some("AliExpress"), has_account: false, has_date: false },
    Prefix { pattern: r"^Alipay ", gateway: Some("Alipay"), has_account: false, has_date: false },
    Prefix { pattern: r"^CKO\*", gateway: Some("Checkout.com"), has_account: false, has_date: false },
    Prefix { pattern: r"^DBS\*", gateway: Some("DBS"), has_account: false, has_date: false },
    Prefix { pattern: r"^DNH\*", gateway: Some("DNH"), has_account: false, has_date: false },
    Prefix { pattern: r"^DOORDASH\*", gateway: Some("DoorDash"), has_account: false, has_date: false },
    Prefix { pattern: r"^EB\s*\*", gateway: Some("Eventbrite"), has_account: false, has_date: false },
    Prefix { pattern: r"^EZI\*", gateway: Some("Ezi"), has_account: false, has_date: false },
    Prefix { pattern: r"^FLEXISCHOOLS\*", gateway: Some("Flexischools"), has_account: false, has_date: false },
    Prefix { pattern: r"^GLOBAL-E\* ", gateway: Some("Global-E"), has_account: false, has_date: false },
    Prefix { pattern: r"^LIGHTSPEED\*(?:SR-)?(?:LS\s+)?", gateway: Some("Lightspeed"), has_account: false, has_date: false },
    Prefix { pattern: r"^LIME\*", gateway: Some("Lime"), has_account: false, has_date: false },
    Prefix { pattern: r"^LS\s+", gateway: Some("Lightspeed"), has_account: false, has_date: false },
    Prefix { pattern: r"^MPASS \*", gateway: Some("mPass"), has_account: false, has_date: false },
    Prefix { pattern: r"^MR YUM\*", gateway: Some("Mr Yum"), has_account: false, has_date: false },
    Prefix { pattern: r"^NAYAXAU\*", gateway: Some("Nayax"), has_account: false, has_date: false },
    Prefix { pattern: r"^PAYPAL \*", gateway: Some("PayPal"), has_account: false, has_date: false },
    Prefix { pattern: r"^PP\*", gateway: Some("PP"), has_account: false, has_date: false },
    Prefix { pattern: r"^(?i:Revolut)\*", gateway: Some("Revolut"), has_account: false, has_date: false },
    Prefix { pattern: r"^SMP\*", gateway: Some("Square Marketplace"), has_account: false, has_date: false },
    Prefix { pattern: r"^SQ \*", gateway: Some("Square"), has_account: false, has_date: false },
    Prefix { pattern: r"^TITHE\.LY\*", gateway: Some("Tithe.ly"), has_account: false, has_date: false },
    Prefix { pattern: r"^TST\*\s*", gateway: Some("Toast"), has_account: false, has_date: false },
    Prefix { pattern: r"^TRYBOOKING\*", gateway: Some("TryBooking"), has_account: false, has_date: false },
    Prefix { pattern: r"^Weixin ", gateway: Some("Weixin"), has_account: false, has_date: false },
    Prefix { pattern: r"^WINDCAVE\*", gateway: Some("Windcave"), has_account: false, has_date: false },
    Prefix { pattern: r"^ZLR\*", gateway: Some("Zeller"), has_account: false, has_date: false },
];

fn data() -> &'static [CompiledPrefix] {
    static COMPILED: OnceLock<Vec<CompiledPrefix>> = OnceLock::new();
    COMPILED.get_or_init(|| {
        PREFIXES
            .iter()
            .map(|p| CompiledPrefix {
                regex: Regex::new(p.pattern).expect("invalid prefix pattern"),
                gateway: p.gateway,
                has_account: p.has_account,
                has_date: p.has_date,
            })
            .collect()
    })
}

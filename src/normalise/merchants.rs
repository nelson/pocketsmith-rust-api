use std::sync::OnceLock;

use regex::Regex;

use super::{NormalisationResult, PayeeClass};

struct Merchant {
    pattern: &'static str,
    canonical: &'static str,
}

struct CompiledMerchant {
    regex: Regex,
    canonical: &'static str,
}

pub fn apply(result: &mut NormalisationResult) {
    if result.class().is_some() {
        return;
    }
    for cm in compiled_merchants() {
        if cm.regex.is_match(&result.normalised) {
            result.features.entity_name = Some(cm.canonical.to_string());
            result.set_class(PayeeClass::Merchant);
            return;
        }
    }
}

// Sorted alphabetically by canonical name.
// Where multiple patterns share a prefix, more specific must appear first.
const MERCHANTS: &[Merchant] = &[
    // --- 7 ---
    Merchant { pattern: r"(?i)7-ELEVEN\b", canonical: "7-Eleven" },
    // --- A ---
    Merchant { pattern: r"(?i)\bAES$", canonical: "AES Electrical" },
    Merchant { pattern: r"(?i)RETURN.*DIRECT DEBIT \d+ AFES", canonical: "AFES (Donation Return)" },
    Merchant { pattern: r"(?i)AFES\b", canonical: "AFES (Donation)" },
    Merchant { pattern: r"(?i)AIRPORT RETAIL ENTER", canonical: "Airport Retail Enterprises" },
    Merchant { pattern: r"(?i)ALDI\b", canonical: "ALDI" },
    Merchant { pattern: r"(?i)AMAZON PRIME", canonical: "Amazon Prime" },
    Merchant { pattern: r"(?i)AMAZON\b", canonical: "Amazon" },
    Merchant { pattern: r"(?i)AMERICAN EXPRESS\b", canonical: "American Express Payment" },
    Merchant { pattern: r"(?i)APPLE\.COM/", canonical: "Apple.com" },
    Merchant { pattern: r"(?i)AMOREPACIFIC", canonical: "Amorepacific" },
    Merchant { pattern: r"(?i)\bATM\b", canonical: "ATM" },
    Merchant { pattern: r"(?i)\bATO\b", canonical: "ATO" },
    Merchant { pattern: r"(?i)AUSKO COOPERATION", canonical: "Ausko Cooperation Concord" },
    Merchant { pattern: r"(?i)AUVERS (?:CAFE|INGENS)", canonical: "Auvers Cafe" },
    // --- B ---
    Merchant { pattern: r"(?i)BAKED BEATS", canonical: "Baked Beats" },
    Merchant { pattern: r"(?i)BAKERS DELIGHT\b", canonical: "Bakers Delight" },
    Merchant { pattern: r"(?i)BEST MART\b", canonical: "Best Mart" },
    Merchant { pattern: r"(?i)\bBP\b", canonical: "BP" },
    Merchant { pattern: r"(?i)BUNNINGS\b", canonical: "Bunnings" },
    Merchant { pattern: r"(?i)BUPA\b", canonical: "BUPA" },
    Merchant { pattern: r"(?i)BURWOOD DISCOUNT CHEMIST", canonical: "Burwood Discount Chemist" },
    Merchant { pattern: r"(?i)BWS\b", canonical: "BWS" },
    // --- C ---
    Merchant { pattern: r"(?i)CAFE SIENNA", canonical: "Cafe Sienna" },
    Merchant { pattern: r"(?i)CAMPION EDUCATION", canonical: "Campion Education" },
    Merchant { pattern: r"(?i)CASH DEPOSIT.*BEEM IT", canonical: "Cash Deposit Beem It" },
    Merchant { pattern: r"(?i)DIRECT CREDIT PENSION (?:XX)?\d+$", canonical: "Centrelink Pension" },
    Merchant { pattern: r"(?i)CHEMIST WAREHOUSE\b", canonical: "Chemist Warehouse" },
    Merchant { pattern: r"(?i)CHILD ASSISTANCE PAYMENT", canonical: "Child Assistance Payment" },
    Merchant { pattern: r"(?i)COLES\b", canonical: "Coles" },
    Merchant { pattern: r"(?i)COMMINSURE\b", canonical: "CommInsure" },
    Merchant { pattern: r"(?i)CORNERSTONE CONCORD", canonical: "Cornerstone Concord" },
    Merchant { pattern: r"(?i)COST OF LIVING", canonical: "Cost of Living Payment" },
    Merchant { pattern: r"(?i)\bCS(?:\s+EDUCATION)?$", canonical: "CS Education Strathfield" },
    // --- D ---
    Merchant { pattern: r"(?i)DAISO\b", canonical: "Daiso" },
    Merchant { pattern: r"(?i)DAVID JONES\b", canonical: "David Jones" },
    Merchant { pattern: r"(?i)DOPA\b", canonical: "Dopa" },
    Merchant { pattern: r"(?i)DOROTHY COWIE SCHOOL", canonical: "Dorothy Cowie School" },
    // --- E ---
    Merchant { pattern: r"(?i)EARLY PURPOSE", canonical: "Early Purpose" },
    Merchant { pattern: r"(?i)EAT ISTANBUL\b", canonical: "Eat Istanbul" },
    Merchant { pattern: r"(?i)ECONOMIC SUPPORT", canonical: "Economic Support Payment" },
    Merchant { pattern: r"(?i)EDITION ROASTERS", canonical: "Edition Roasters" },
    Merchant { pattern: r"(?i)EG FUELCO\b", canonical: "EG Fuelco" },
    Merchant { pattern: r"(?i)ENERGYAUSTRALIA", canonical: "EnergyAustralia" },
    // --- F ---
    Merchant { pattern: r"(?i)FLOWER POWER\b", canonical: "Flower Power" },
    Merchant { pattern: r"(?i)\bFMC$", canonical: "FMC" },
    // --- G ---
    Merchant { pattern: r"(?i)GENESIS GARDENS", canonical: "Genesis Gardens" },
    Merchant { pattern: r"(?i)GNT SERVICES", canonical: "GNT Services" },
    Merchant { pattern: r"(?i)GONG CHA\b", canonical: "Gong Cha" },
    Merchant { pattern: r"(?i)GOOD VEN(?:TURE|TRE)", canonical: "Good Venture Partners" },
    Merchant { pattern: r"(?i)GREENWAY MEAT\b", canonical: "Greenway Meat" },
    Merchant { pattern: r"(?i)GUMPTION COFFEE", canonical: "Gumption Coffee" },
    Merchant { pattern: r"(?i)GUZMAN Y GOMEZ\b", canonical: "Guzman Y Gomez" },
    // --- H ---
    Merchant { pattern: r"(?i)HAN SANG\b", canonical: "Han Sang" },
    Merchant { pattern: r"(?i)\bHCF\b", canonical: "HCF Health" },
    Merchant { pattern: r"(?i)HEALTHY CARE\b", canonical: "Healthy Care" },
    Merchant { pattern: r"(?i)HERITAGE COFFEE", canonical: "Heritage Coffee" },
    Merchant { pattern: r"(?i)HERO SUSHI\b", canonical: "Hero Sushi" },
    Merchant { pattern: r"(?i)HOLLARD(?:INS|\s+INSURANCE)", canonical: "Hollard Insurance" },
    Merchant { pattern: r"(?i)HONG KONG BING SUTT", canonical: "Hong Kong Bing Sutt" },
    Merchant { pattern: r"(?i)HUBHELLO", canonical: "Hubhello" },
    // --- I ---
    Merchant { pattern: r"(?i)IFTTT STRAVA ACTIVITY", canonical: "IFTTT Strava" },
    Merchant { pattern: r"(?i)\bING$", canonical: "ING" },
    Merchant { pattern: r"(?i)INTERPARK UTS", canonical: "Interpark UTS" },
    Merchant { pattern: r"(?i)IVY.*MUMU", canonical: "Ivy Mumu" },
    // --- J ---
    Merchant { pattern: r"(?i)JASON HUI?$", canonical: "Jason Hui" },
    Merchant { pattern: r"(?i)JFC\b", canonical: "JFC" },
    Merchant { pattern: r"(?i)(?:PAYMENT|TRANSFER) FROM TAM S", canonical: "Joint Account (Tam)" },
    // --- K ---
    Merchant { pattern: r"(?i)KFC\b", canonical: "KFC" },
    Merchant { pattern: r"(?i)KMART\b", canonical: "Kmart" },
    Merchant { pattern: r"(?i)KOMART\b", canonical: "Komart" },
    // --- L ---
    Merchant { pattern: r"(?i)LEIBLE COFFEE", canonical: "Leible Coffee" },
    Merchant { pattern: r"(?i)LINKT\b", canonical: "Linkt" },
    Merchant { pattern: r"(?i)LUNEBURGER\b", canonical: "Luneburger" },
    // --- M ---
    Merchant { pattern: r"(?i)MACHI\b", canonical: "Machi" },
    Merchant { pattern: r"(?i)MACQUARIE UNIVERSITY", canonical: "Macquarie University" },
    Merchant { pattern: r"(?i)MAENAM LAO\b", canonical: "Maenam Lao" },
    Merchant { pattern: r"(?i)MANCINI.?S (?:PIZZERIA|WOOD)", canonical: "Mancini's" },
    Merchant { pattern: r"(?i)MARRICKVILLE PORK ROLL", canonical: "Marrickville Pork Roll" },
    Merchant { pattern: r"(?i)MCDONALD'S\b", canonical: "McDonald's" },
    Merchant { pattern: r"(?i)MEDICARE BENEFITS", canonical: "Medicare Benefits" },
    Merchant { pattern: r"(?i)MEET FRESH\b", canonical: "Meet Fresh" },
    Merchant { pattern: r"(?i)MEMOCORP AUSTRALIA", canonical: "Memocorp Australia" },
    Merchant { pattern: r"(?i)MINISO\b", canonical: "Miniso" },
    // --- N ---
    Merchant { pattern: r"(?i)NDIS NSW", canonical: "NDIS Payment" },
    Merchant { pattern: r"(?i)NEWYEN INVESTMENT", canonical: "Newyen Investment" },
    // --- O ---
    Merchant { pattern: r"(?i)OFFICEWORKS\b", canonical: "Officeworks" },
    Merchant { pattern: r"(?i)OILSTONE", canonical: "Oilstone" },
    Merchant { pattern: r"(?i)\bOMF(?:\s+INTERNATIONAL)?$", canonical: "OMF International" },
    // --- P ---
    Merchant { pattern: r"(?i)PAPPARICH\b", canonical: "Papparich" },
    Merchant { pattern: r"(?i)PAYPAL \*BUDGETPETPR", canonical: "PayPal Budget Pet Products" },
    Merchant { pattern: r"(?i)PIONEERS OF(?:\s+AUST(?:RALIA)?)?$", canonical: "Pioneers of Australia" },
    Merchant { pattern: r"(?i)POP MART\b", canonical: "Pop Mart" },
    Merchant { pattern: r"(?i)PRICELINE PHARMACY\b", canonical: "Priceline Pharmacy" },
    // --- R ---
    Merchant { pattern: r"(?i)REGIMENT SPECIAL(?:I?TY|TY) (?:COF(?:FEE?)?|CAF)", canonical: "Regiment Coffee" },
    // --- S ---
    Merchant { pattern: r"(?i)SERVICE NSW\b", canonical: "Service NSW" },
    Merchant { pattern: r"(?i)SLOWER DEEPER WISER", canonical: "Slower Deeper Wiser" },
    Merchant { pattern: r"(?i)SOULGRAM PARTNERS", canonical: "Soulgram Partners" },
    Merchant { pattern: r"(?i)STARBUCKS\b", canonical: "Starbucks" },
    Merchant { pattern: r"(?i)STRATHFIELD COUNCIL", canonical: "Strathfield Council" },
    Merchant { pattern: r"(?i)SUSHI HUB\b", canonical: "Sushi Hub" },
    Merchant { pattern: r"(?i)SUSHI NAYA\b", canonical: "Sushi Naya" },
    Merchant { pattern: r"(?i)SUSHI WORLD\b", canonical: "Sushi World" },
    Merchant { pattern: r"(?i)SYDNEY WATER", canonical: "Sydney Water" },
    // --- T ---
    Merchant { pattern: r"(?i)TAN HANDS", canonical: "Tan Hands Physiotherapy" },
    Merchant { pattern: r"(?i)TARGET\b", canonical: "Target" },
    Merchant { pattern: r"(?i)TEA SPOT\b", canonical: "Tea Spot" },
    Merchant { pattern: r"(?i)THE AVENUE.*NEWINGTON", canonical: "The Avenue Newington" },
    Merchant { pattern: r"(?i)THE LOCAL ENFIELD", canonical: "The Local Enfield" },
    Merchant { pattern: r"(?i)TRANSPORT\s*(?:FOR\s*)?NSW", canonical: "Transport for NSW" },
    // --- U ---
    Merchant { pattern: r"(?i)UBER\s*\*?\s*EATS\b", canonical: "Uber Eats" },
    Merchant { pattern: r"(?i)UBER\s*\*?\s*TRIP\b", canonical: "Uber Trip" },
    Merchant { pattern: r"(?i)UBER\b", canonical: "Uber" },
    // --- V ---
    Merchant { pattern: r"(?i)VISCO UNIVERSAL", canonical: "Visco Universal" },
    Merchant { pattern: r"(?i)VN CITY\b", canonical: "VN City" },
    // --- W ---
    Merchant { pattern: r"(?i)WOOLWORTHS\b", canonical: "Woolworths" },
    // --- Y ---
    Merchant { pattern: r"(?i)YAKITORI JIN", canonical: "Yakitori Jin" },
];

fn compiled_merchants() -> &'static [CompiledMerchant] {
    static COMPILED: OnceLock<Vec<CompiledMerchant>> = OnceLock::new();
    COMPILED.get_or_init(|| {
        MERCHANTS
            .iter()
            .map(|m| CompiledMerchant {
                regex: Regex::new(m.pattern).expect("invalid merchant pattern"),
                canonical: m.canonical,
            })
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_woolworths() {
        let mut r = NormalisationResult::new("WOOLWORTHS 1624 STRATHFIELD");
        apply(&mut r);
        assert_eq!(r.features.entity_name.as_deref(), Some("Woolworths"));
        assert_eq!(r.class(), Some(&PayeeClass::Merchant));
    }

    #[test]
    fn test_afes_standalone() {
        let mut r = NormalisationResult::new("AFES");
        apply(&mut r);
        assert_eq!(r.features.entity_name.as_deref(), Some("AFES (Donation)"));
        assert_eq!(r.class(), Some(&PayeeClass::Merchant));
    }

    #[test]
    fn test_transfer_to_afes() {
        let mut r = NormalisationResult::new("TRANSFER TO AFES");
        apply(&mut r);
        assert_eq!(r.features.entity_name.as_deref(), Some("AFES (Donation)"));
    }

    #[test]
    fn test_comminsure_post_prefix() {
        let mut r = NormalisationResult::new("COMMINSURE");
        apply(&mut r);
        assert_eq!(r.features.entity_name.as_deref(), Some("CommInsure"));
    }

    #[test]
    fn test_centrelink_pension() {
        let mut r = NormalisationResult::new("DIRECT CREDIT PENSION XX1234");
        apply(&mut r);
        assert_eq!(r.features.entity_name.as_deref(), Some("Centrelink Pension"));
    }

    #[test]
    fn test_skip_if_classified() {
        let mut r = NormalisationResult::new("WOOLWORTHS");
        r.set_class(PayeeClass::Person);
        apply(&mut r);
        assert!(r.features.entity_name.is_none());
    }

    #[test]
    fn test_case_insensitive() {
        let mut r = NormalisationResult::new("woolworths");
        apply(&mut r);
        assert_eq!(r.features.entity_name.as_deref(), Some("Woolworths"));
    }

    #[test]
    fn test_transport_nsw_no_spaces() {
        let mut r = NormalisationResult::new("TRANSPORTFORNSWTRAVEL SYDNEY");
        apply(&mut r);
        assert_eq!(r.features.entity_name.as_deref(), Some("Transport for NSW"));
    }

    #[test]
    fn test_transport_nsw_opal() {
        let mut r = NormalisationResult::new("TRANSPORT FOR NSW-OPAL HAYMARKET");
        apply(&mut r);
        assert_eq!(r.features.entity_name.as_deref(), Some("Transport for NSW"));
    }

    #[test]
    fn test_apple_com_bill() {
        let mut r = NormalisationResult::new("APPLE.COM/BILL SYDNEY");
        apply(&mut r);
        assert_eq!(r.features.entity_name.as_deref(), Some("Apple.com"));
    }

    #[test]
    fn test_apple_com_au() {
        let mut r = NormalisationResult::new("APPLE.COM/AU SYDNEY");
        apply(&mut r);
        assert_eq!(r.features.entity_name.as_deref(), Some("Apple.com"));
    }

    #[test]
    fn test_edition_roasters() {
        let mut r = NormalisationResult::new("EDITION ROASTERS WYN 99 SYDNEY");
        apply(&mut r);
        assert_eq!(r.features.entity_name.as_deref(), Some("Edition Roasters"));
    }

    #[test]
    fn test_the_local_enfield() {
        let mut r = NormalisationResult::new("THE LOCAL ENFIELD Croydon Park");
        apply(&mut r);
        assert_eq!(r.features.entity_name.as_deref(), Some("The Local Enfield"));
    }

    #[test]
    fn test_best_mart() {
        let mut r = NormalisationResult::new("BEST MART STRATHFIELD P STRATHFIELD");
        apply(&mut r);
        assert_eq!(r.features.entity_name.as_deref(), Some("Best Mart"));
    }

    #[test]
    fn test_amazon_marketplace() {
        let mut r = NormalisationResult::new("AMAZON MARKETPLACE AU SYDNEY SOUTH");
        apply(&mut r);
        assert_eq!(r.features.entity_name.as_deref(), Some("Amazon"));
    }

    #[test]
    fn test_amazon_au() {
        let mut r = NormalisationResult::new("AMAZON AU SYDNEY SOUTH");
        apply(&mut r);
        assert_eq!(r.features.entity_name.as_deref(), Some("Amazon"));
    }

    #[test]
    fn test_amazon_prime_still_works() {
        let mut r = NormalisationResult::new("AMAZON PRIME AU");
        apply(&mut r);
        assert_eq!(r.features.entity_name.as_deref(), Some("Amazon Prime"));
    }

    #[test]
    fn test_regiment_speciality_truncated() {
        let mut r = NormalisationResult::new("REGIMENT SPECIALITY CAF Sydney");
        apply(&mut r);
        assert_eq!(r.features.entity_name.as_deref(), Some("Regiment Coffee"));
    }

    #[test]
    fn test_regiment_specialty_coffee() {
        let mut r = NormalisationResult::new("REGIMENT SPECIALTY COFFEE Sydney");
        apply(&mut r);
        assert_eq!(r.features.entity_name.as_deref(), Some("Regiment Coffee"));
    }

    #[test]
    fn test_gumption_coffee() {
        let mut r = NormalisationResult::new("GUMPTION COFFEE Sydney");
        apply(&mut r);
        assert_eq!(r.features.entity_name.as_deref(), Some("Gumption Coffee"));
    }

    #[test]
    fn test_uber_trip() {
        let mut r = NormalisationResult::new("UBER TRIP HELP.UBER.COM");
        apply(&mut r);
        assert_eq!(r.features.entity_name.as_deref(), Some("Uber Trip"));
    }

    #[test]
    fn test_uber_star_trip() {
        let mut r = NormalisationResult::new("UBER *TRIP Sydney AU AUS");
        apply(&mut r);
        assert_eq!(r.features.entity_name.as_deref(), Some("Uber Trip"));
    }

    #[test]
    fn test_uber_eats() {
        let mut r = NormalisationResult::new("UBER EATS HELP.UBER.COM");
        apply(&mut r);
        assert_eq!(r.features.entity_name.as_deref(), Some("Uber Eats"));
    }

    #[test]
    fn test_uber_star_eats() {
        let mut r = NormalisationResult::new("UBER *EATS Sydney AU AUS");
        apply(&mut r);
        assert_eq!(r.features.entity_name.as_deref(), Some("Uber Eats"));
    }

    #[test]
    fn test_paypal_ubereats() {
        let mut r = NormalisationResult::new("UBEREATS AU");
        apply(&mut r);
        assert_eq!(r.features.entity_name.as_deref(), Some("Uber Eats"));
    }

    #[test]
    fn test_uber_australia_refund() {
        let mut r = NormalisationResult::new("Uber Australia Pty Ltd");
        apply(&mut r);
        assert_eq!(r.features.entity_name.as_deref(), Some("Uber"));
    }

    #[test]
    fn test_uber_au_paypal_stripped() {
        let mut r = NormalisationResult::new("UBER AU");
        apply(&mut r);
        assert_eq!(r.features.entity_name.as_deref(), Some("Uber"));
    }

    #[test]
    fn test_marrickville_pork_roll() {
        let mut r = NormalisationResult::new("MARRICKVILLE PORK ROLL Sydney");
        apply(&mut r);
        assert_eq!(r.features.entity_name.as_deref(), Some("Marrickville Pork Roll"));
    }
}

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
    Merchant { pattern: r"(?i)APPLE PARK CAFFE", canonical: "Apple Park Caffe" },
    Merchant { pattern: r"(?i)APPLE\.COM/", canonical: "Apple.com" },
    Merchant { pattern: r"(?i)AMOREPACIFIC", canonical: "Amorepacific" },
    Merchant { pattern: r"(?i)\bATM\b", canonical: "ATM" },
    Merchant { pattern: r"(?i)\bATO\b", canonical: "ATO" },
    Merchant { pattern: r"(?i)AUSSIE BROADBAND", canonical: "Aussie Broadband" },
    Merchant { pattern: r"(?i)AUSKO COOPERATION", canonical: "Ausko Cooperation Concord" },
    Merchant { pattern: r"(?i)AUVERS (?:CAFE|INGENS)", canonical: "Auvers Cafe" },
    // --- B ---
    Merchant { pattern: r"(?i)BAKED BEATS", canonical: "Baked Beats" },
    Merchant { pattern: r"(?i)BAKERS DELIGHT\b", canonical: "Bakers Delight" },
    Merchant { pattern: r"(?i)BEST MART\b", canonical: "Best Mart" },
    Merchant { pattern: r"(?i)\bBP\b", canonical: "BP" },
    Merchant { pattern: r"(?i)BROADWAYSHOPPINGCENT", canonical: "Broadway Shopping Centre Carpark" },
    Merchant { pattern: r"(?i)BUNNINGS\b", canonical: "Bunnings" },
    Merchant { pattern: r"(?i)BUPA\b", canonical: "BUPA" },
    Merchant { pattern: r"(?i)BURWOOD DISCOUNT CHEMIST", canonical: "Burwood Discount Chemist" },
    Merchant { pattern: r"(?i)BWS\b", canonical: "BWS" },
    // --- C ---
    Merchant { pattern: r"(?i)CAFE SIENNA", canonical: "Cafe Sienna" },
    Merchant { pattern: r"(?i)CAMPION EDUCATION", canonical: "Campion Education" },
    Merchant { pattern: r"(?i)DIRECT CREDIT PENSION (?:XX)?\d+$", canonical: "Centrelink Pension" },
    Merchant { pattern: r"(?i)CHEMIST WAREHOUSE\b", canonical: "Chemist Warehouse" },
    Merchant { pattern: r"(?i)CHILD ASSISTANCE PAYMENT", canonical: "Child Assistance Payment" },
    Merchant { pattern: r"(?i)COLES\b", canonical: "Coles" },
    Merchant { pattern: r"(?i)COMMINSURE\b", canonical: "CommInsure" },
    Merchant { pattern: r"(?i)COMPASSION AUSTRALIA", canonical: "Compassion Australia" },
    Merchant { pattern: r"(?i)CORNERSTONE CAFE", canonical: "Cornerstone Cafe UTS" },
    Merchant { pattern: r"(?i)CORNERSTONE CONCORD", canonical: "Cornerstone Concord" },
    Merchant { pattern: r"(?i)COSTIS FISH AND CHIPS", canonical: "Costis Fish & Chips" },
    Merchant { pattern: r"(?i)COST OF LIVING", canonical: "Cost of Living Payment" },
    Merchant { pattern: r"(?i)CULT EATERY", canonical: "Cult Eatery" },
    Merchant { pattern: r"(?i)\bCS(?:\s+EDUCATION)?$", canonical: "CS Education Strathfield" },
    // --- D ---
    Merchant { pattern: r"(?i)DAISO\b", canonical: "Daiso" },
    Merchant { pattern: r"(?i)DAVID JONES\b", canonical: "David Jones" },
    Merchant { pattern: r"(?i)DIGGY DOO['\u{2019}]?S?\b", canonical: "Diggy Doo's Coffee" },
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
    Merchant { pattern: r"(?i)FRESH SEAFOOD", canonical: "Fresh Seafood Strathfield" },
    // --- G ---
    Merchant { pattern: r"(?i)GENESIS GARDENS", canonical: "Genesis Gardens" },
    Merchant { pattern: r"(?i)GNT SERVICES", canonical: "GNT Services" },
    Merchant { pattern: r"(?i)GONG CHA\b", canonical: "Gong Cha" },
    Merchant { pattern: r"(?i)GOOD VEN(?:TURE|TRE)", canonical: "Good Venture Partners" },
    Merchant { pattern: r"(?i)GR BUY\b", canonical: "GR Buy Supermarket" },
    Merchant { pattern: r"(?i)GREENWAY MEAT\b", canonical: "Greenway Meat" },
    Merchant { pattern: r"(?i)GU HEALTH\b", canonical: "GU Health" },
    Merchant { pattern: r"(?i)GUMPTION COFFEE", canonical: "Gumption Coffee" },
    Merchant { pattern: r"(?i)GUZMAN Y GOMEZ\b", canonical: "Guzman Y Gomez" },
    // --- H ---
    Merchant { pattern: r"(?i)HAN SANG\b", canonical: "Han Sang" },
    Merchant { pattern: r"(?i)\bHCF(?:HEALTH)?\b", canonical: "HCF Health" },
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
    Merchant { pattern: r"(?i)J YANG\b", canonical: "J Yang" }, // Massage
    Merchant { pattern: r"(?i)JAEKYUN PARK\b", canonical: "Jaekyun Park" }, // Cleaning
    Merchant { pattern: r"(?i)JFC\b", canonical: "JFC" },
    Merchant { pattern: r"(?i)(?:PAYMENT|TRANSFER) FROM TAM S", canonical: "Joint Account (Tam)" },
    // --- K ---
    Merchant { pattern: r"(?i)KFC\b", canonical: "KFC" },
    Merchant { pattern: r"(?i)KIM SUN YOUNG HAIR", canonical: "Kim Sun Young Hair" },
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
    Merchant { pattern: r"(?i)MAMAK(?:S(?:MLC)?|(?:\s+VILLAGE))", canonical: "Mamak" },
    Merchant { pattern: r"(?i)MANCINI.?S (?:PIZZERIA|WOOD)", canonical: "Mancini's" },
    Merchant { pattern: r"(?i)MARRICKVILLE PORK ROLL", canonical: "Marrickville Pork Roll" },
    Merchant { pattern: r"(?i)MCM SOUP\b", canonical: "MCM Soup" },
    Merchant { pattern: r"(?i)MCDONALD'?S\b", canonical: "McDonald's" },
    Merchant { pattern: r"(?i)MEDICARE BENEFITS", canonical: "Medicare Benefits" },
    Merchant { pattern: r"(?i)MEET FRESH\b", canonical: "Meet Fresh" },
    Merchant { pattern: r"(?i)MEMOCORP AUSTRALIA", canonical: "Memocorp Australia" },
    Merchant { pattern: r"(?i)MERIDEN SCHOOL", canonical: "Meriden School" },
    Merchant { pattern: r"(?i)MINISO\b", canonical: "Miniso" },
    Merchant { pattern: r"(?i)MONKEY HOUSE\b", canonical: "Monkey House" },
    // --- N ---
    Merchant { pattern: r"(?i)NDIS NSW", canonical: "NDIS Payment" },
    Merchant { pattern: r"(?i)NETFLIX\b", canonical: "Netflix" },
    Merchant { pattern: r"(?i)NEWYEN INVESTMENT", canonical: "Newyen Investment" },
    // --- O ---
    Merchant { pattern: r"(?i)OFFICEWORKS\b", canonical: "Officeworks" },
    Merchant { pattern: r"(?i)OILSTONE", canonical: "Oilstone" },
    Merchant { pattern: r"(?i)\bOMF(?:\s+INTERNATIONA?L?)?\b", canonical: "OMF International" },
    // --- P ---
    Merchant { pattern: r"(?i)PAPPARICH\b", canonical: "Papparich" },
    Merchant { pattern: r"(?i)PARKNPAY\b", canonical: "ParkNPay" },
    Merchant { pattern: r"(?i)PASTA PANTRY", canonical: "Pasta Pantry MLC" },
    Merchant { pattern: r"(?i)PAYPAL \*BUDGETPETPR", canonical: "PayPal Budget Pet Products" },
    Merchant { pattern: r"(?i)PIONEERS OF(?:\s+AUST(?:RALIA)?)?$", canonical: "Pioneers of Australia" },
    Merchant { pattern: r"(?i)POP MART\b", canonical: "Pop Mart" },
    Merchant { pattern: r"(?i)POWERSHOP", canonical: "Powershop" },
    Merchant { pattern: r"(?i)PRICELINE PHARMACY\b", canonical: "Priceline Pharmacy" },
    // --- Q ---
    Merchant { pattern: r"(?i)QANTAS\b", canonical: "Qantas" },
    // --- R ---
    Merchant { pattern: r"(?i)REGIMENT SPECIAL(?:I?TY|TY) (?:COF(?:FEE?)?|CAF)", canonical: "Regiment Coffee" },
    // --- S ---
    Merchant { pattern: r"(?i)SERVICE NSW\b", canonical: "Service NSW" },
    Merchant { pattern: r"(?i)SLOWER DEEPER WISER", canonical: "Slower Deeper Wiser" },
    Merchant { pattern: r"(?i)SOULGRAM PARTNERS", canonical: "Soulgram Partners" },
    Merchant { pattern: r"(?i)STARBUCKS\b", canonical: "Starbucks" },
    Merchant { pattern: r"(?i)STITCH COFFEE", canonical: "Stitch Coffee" },
    Merchant { pattern: r"(?i)STOCK MARKET KITCHEN", canonical: "Stock Market Kitchen" },
    Merchant { pattern: r"(?i)STRATHFIELD COUNCIL", canonical: "Strathfield Council" },
    Merchant { pattern: r"(?i)SUSHI HUB\b", canonical: "Sushi Hub" },
    Merchant { pattern: r"(?i)SUSHI NAYA\b", canonical: "Sushi Naya" },
    Merchant { pattern: r"(?i)SUSHI WORLD\b", canonical: "Sushi World" },
    Merchant { pattern: r"(?i)SYDNEY WATER", canonical: "Sydney Water" },
    // --- T ---
    Merchant { pattern: r"(?i)TAN HANDS", canonical: "Tan Hands Physiotherapy" },
    Merchant { pattern: r"(?i)TARGET\b", canonical: "Target" },
    Merchant { pattern: r"(?i)TEA SPOT\b", canonical: "Tea Spot" },
    Merchant { pattern: r"(?i)TENCENT\b", canonical: "Tencent" },
    Merchant { pattern: r"(?i)TELSTRA\b", canonical: "Telstra" },
    Merchant { pattern: r"(?i)THE AVENUE.*NEWINGTON", canonical: "The Avenue Newington" },
    Merchant { pattern: r"(?i)THE LOCAL ENFIELD", canonical: "The Local Enfield" },
    Merchant { pattern: r"(?i)THE MANDOO DUMPLING", canonical: "The Mandoo Dumpling" },
    Merchant { pattern: r"(?i)TRANSPORT\s*(?:FOR\s*)?NSW", canonical: "Transport for NSW" },
    // --- U ---
    Merchant { pattern: r"(?i)UBER\s*\*?\s*EATS\b", canonical: "Uber Eats" },
    Merchant { pattern: r"(?i)UBER\s*\*?\s*TRIP\b", canonical: "Uber Trip" },
    Merchant { pattern: r"(?i)UBER\b", canonical: "Uber" },
    Merchant { pattern: r"(?i)UNIQLO\b", canonical: "Uniqlo" },
    // --- V ---
    Merchant { pattern: r"(?i)VISCO UNIVERSAL", canonical: "Visco Universal" },
    Merchant { pattern: r"(?i)VN CITY\b", canonical: "VN City" },
    Merchant { pattern: r"(?i)VODAFONE\b", canonical: "Vodafone" },
    // --- W ---
    Merchant { pattern: r"(?i)WISE AUSTRALIA\b", canonical: "Wise" },
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

    fn assert_merchant(input: &str, expected: &str) {
        let mut r = NormalisationResult::new(input);
        apply(&mut r);
        assert_eq!(r.features.entity_name.as_deref(), Some(expected));
    }

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
    fn test_skip_if_classified() {
        let mut r = NormalisationResult::new("WOOLWORTHS");
        r.set_class(PayeeClass::Person);
        apply(&mut r);
        assert!(r.features.entity_name.is_none());
    }

    #[test]
    fn test_case_insensitive() {
        assert_merchant("woolworths", "Woolworths");
    }

    #[test]
    fn test_transfer_to_afes() {
        assert_merchant("TRANSFER TO AFES", "AFES (Donation)");
    }

    #[test]
    fn test_comminsure_post_prefix() {
        assert_merchant("COMMINSURE", "CommInsure");
    }

    #[test]
    fn test_centrelink_pension() {
        assert_merchant("DIRECT CREDIT PENSION XX1234", "Centrelink Pension");
    }

    #[test]
    fn test_transport_nsw_no_spaces() {
        assert_merchant("TRANSPORTFORNSWTRAVEL SYDNEY", "Transport for NSW");
    }

    #[test]
    fn test_transport_nsw_opal() {
        assert_merchant("TRANSPORT FOR NSW-OPAL HAYMARKET", "Transport for NSW");
    }

    #[test]
    fn test_apple_com_bill() {
        assert_merchant("APPLE.COM/BILL SYDNEY", "Apple.com");
    }

    #[test]
    fn test_apple_com_au() {
        assert_merchant("APPLE.COM/AU SYDNEY", "Apple.com");
    }

    #[test]
    fn test_edition_roasters() {
        assert_merchant("EDITION ROASTERS WYN 99 SYDNEY", "Edition Roasters");
    }

    #[test]
    fn test_the_local_enfield() {
        assert_merchant("THE LOCAL ENFIELD Croydon Park", "The Local Enfield");
    }

    #[test]
    fn test_best_mart() {
        assert_merchant("BEST MART STRATHFIELD P STRATHFIELD", "Best Mart");
    }

    #[test]
    fn test_amazon_marketplace() {
        assert_merchant("AMAZON MARKETPLACE AU SYDNEY SOUTH", "Amazon");
    }

    #[test]
    fn test_amazon_au() {
        assert_merchant("AMAZON AU SYDNEY SOUTH", "Amazon");
    }

    #[test]
    fn test_amazon_prime_still_works() {
        assert_merchant("AMAZON PRIME AU", "Amazon Prime");
    }

    #[test]
    fn test_regiment_speciality_truncated() {
        assert_merchant("REGIMENT SPECIALITY CAF Sydney", "Regiment Coffee");
    }

    #[test]
    fn test_regiment_specialty_coffee() {
        assert_merchant("REGIMENT SPECIALTY COFFEE Sydney", "Regiment Coffee");
    }

    #[test]
    fn test_gumption_coffee() {
        assert_merchant("GUMPTION COFFEE Sydney", "Gumption Coffee");
    }

    #[test]
    fn test_uber_trip() {
        assert_merchant("UBER TRIP HELP.UBER.COM", "Uber Trip");
    }

    #[test]
    fn test_uber_star_trip() {
        assert_merchant("UBER *TRIP Sydney AU AUS", "Uber Trip");
    }

    #[test]
    fn test_uber_eats() {
        assert_merchant("UBER EATS HELP.UBER.COM", "Uber Eats");
    }

    #[test]
    fn test_uber_star_eats() {
        assert_merchant("UBER *EATS Sydney AU AUS", "Uber Eats");
    }

    #[test]
    fn test_paypal_ubereats() {
        assert_merchant("UBEREATS AU", "Uber Eats");
    }

    #[test]
    fn test_uber_australia_refund() {
        assert_merchant("Uber Australia Pty Ltd", "Uber");
    }

    #[test]
    fn test_uber_au_paypal_stripped() {
        assert_merchant("UBER AU", "Uber");
    }

    #[test]
    fn test_marrickville_pork_roll() {
        assert_merchant("MARRICKVILLE PORK ROLL Sydney", "Marrickville Pork Roll");
    }

    #[test]
    fn test_broadway_shopping_centre() {
        assert_merchant("THEBROADWAYSHOPPINGCENT BROADWAY", "Broadway Shopping Centre Carpark");
    }

    #[test]
    fn test_diggy_doos_apostrophe() {
        assert_merchant("DIGGY DOO'S COFFEE PTY Sydney", "Diggy Doo's Coffee");
    }

    #[test]
    fn test_diggy_doos_no_apostrophe() {
        assert_merchant("DIGGY DOOS COFFEE Sydney", "Diggy Doo's Coffee");
    }

    #[test]
    fn test_fresh_seafood() {
        assert_merchant("FRESH SEAFOOD STRATHFIELD Strathfield", "Fresh Seafood Strathfield");
    }

    #[test]
    fn test_gr_buy_supermarket() {
        assert_merchant("GR BUY ASIAN SUPERMARKET BURWOOD", "GR Buy Supermarket");
    }

    #[test]
    fn test_meriden_school() {
        assert_merchant("MERIDEN SCHOOL STRATHFIELD STRATHFIELD", "Meriden School");
    }

    #[test]
    fn test_netflix_dot_com() {
        assert_merchant("NETFLIX.COM MELBOURNE", "Netflix");
    }

    #[test]
    fn test_netflix_australia() {
        assert_merchant("NETFLIX AUSTRALIA PTY L MELBOURNE", "Netflix");
    }

    #[test]
    fn test_parknpay() {
        assert_merchant("PARKNPAY NSW*BURWOOD ST LEONARDS", "ParkNPay");
    }

    #[test]
    fn test_pasta_pantry() {
        assert_merchant("PASTA PANTRY MLC KIRRAWEE", "Pasta Pantry MLC");
    }

    #[test]
    fn test_powershop() {
        assert_merchant("POWERSHOPAUSTRALIA POWE MELBOURNE", "Powershop");
    }

    #[test]
    fn test_stitch_coffee_broadway() {
        assert_merchant("STITCH COFFEE BROADWAY Haymarket", "Stitch Coffee");
    }

    #[test]
    fn test_stitch_coffee_ultimo() {
        assert_merchant("STITCH COFFEE ULTIMO", "Stitch Coffee");
    }

    #[test]
    fn test_stock_market_kitchen() {
        assert_merchant("STOCK MARKET KITCHEN 25 Sydney", "Stock Market Kitchen");
    }

    #[test]
    fn test_vodafone_australia() {
        assert_merchant("VODAFONE AUSTRALIA North Sydney", "Vodafone");
    }

    #[test]
    fn test_vodafone_star() {
        assert_merchant("VODAFONE *AUSTRALIA NORTH SYDNEY", "Vodafone");
    }

    #[test]
    fn test_apple_park_caffe() {
        assert_merchant("APPLE PARK CAFFE EB C00 CUPERTINO", "Apple Park Caffe");
    }

    #[test]
    fn test_aussie_broadband() {
        assert_merchant("Aussie Broadband limited", "Aussie Broadband");
    }

    #[test]
    fn test_compassion_australia() {
        assert_merchant("COMPASSION AUSTRALIA WARABROOK", "Compassion Australia");
    }

    #[test]
    fn test_j_yang_cleaning() {
        assert_merchant("Transfer To J YANG, PayID Phone from CommBank App, Massage", "J Yang");
    }

    #[test]
    fn test_jaekyun_park_cleaning() {
        assert_merchant("Transfer To Jaekyun Park, CommBank App Cleaning", "Jaekyun Park");
    }

    #[test]
    fn test_omf_international_with_suffix() {
        assert_merchant("OMF INTERNATIONAL, 21231", "OMF International");
    }

    #[test]
    fn test_qantas() {
        assert_merchant("QANTAS MASCOT", "Qantas");
    }

    #[test]
    fn test_tencent() {
        assert_merchant("TENCENT SHENZHEN", "Tencent");
    }

    #[test]
    fn test_the_mandoo_dumpling() {
        assert_merchant("THE MANDOO DUMPLING STRATHFIELD", "The Mandoo Dumpling");
    }

    #[test]
    fn test_cornerstone_cafe_uts() {
        assert_merchant("CORNERSTONE CAFE UTS ULTIMO", "Cornerstone Cafe UTS");
    }

    #[test]
    fn test_cornerstone_cafe_haymarket() {
        assert_merchant("CORNERSTONE CAFE UTS Haymarket", "Cornerstone Cafe UTS");
    }

    #[test]
    fn test_costis_fish_and_chips() {
        assert_merchant("COSTIS FISH AND CHIPS 1 SYDNEY", "Costis Fish & Chips");
    }

    #[test]
    fn test_cult_eatery() {
        assert_merchant("CULT EATERY NORTH RYDE", "Cult Eatery");
    }

    #[test]
    fn test_gu_health() {
        assert_merchant("GU HEALTH NEWCASTLE", "GU Health");
    }

    #[test]
    fn test_hcf_health_no_space() {
        assert_merchant("HCFHEALTH SYDNEY SYDNEY", "HCF Health");
    }

    #[test]
    fn test_kim_sun_young_hair() {
        assert_merchant("KIM SUN YOUNG HAIR PTY SYDNEY", "Kim Sun Young Hair");
    }

    #[test]
    fn test_mamaks_mlc() {
        assert_merchant("MAMAKSMLC XX2906 SYDNEY", "Mamak");
    }

    #[test]
    fn test_mamak_village() {
        assert_merchant("MAMAK VILLAGE MLC SYDNEY", "Mamak");
    }

    #[test]
    fn test_mcm_soup() {
        assert_merchant("MCM SOUP PTY LTD BURWOOD", "MCM Soup");
    }

    #[test]
    fn test_mcdonalds_no_apostrophe() {
        assert_merchant("MCDONALDS WYNYARD RAIL SYDNEY", "McDonald's");
    }

    #[test]
    fn test_monkey_house() {
        assert_merchant("MONKEY HOUSE STRATHFIELD STRATHFIELD", "Monkey House");
    }

    #[test]
    fn test_telstra() {
        assert_merchant("TELSTRA RECURRING PAYME MELBOURNE", "Telstra");
    }

    #[test]
    fn test_uniqlo() {
        assert_merchant("UNIQLO AUSTRALIA PTY LT MELBOURNE", "Uniqlo");
    }

    #[test]
    fn test_wise_australia() {
        assert_merchant("To Wise Australia Pty Ltd -", "Wise");
    }
}

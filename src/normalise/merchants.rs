use regex::Regex;
use std::sync::OnceLock;

struct CompiledMerchant {
    regex: Regex,
    canonical: &'static str,
}

struct KnownMerchantPattern {
    pattern: &'static str,
    canonical: &'static str,
}

const KNOWN_MERCHANT_PATTERNS: &[KnownMerchantPattern] = &[
    // Banking identity mappings (higher priority — matched first)
    KnownMerchantPattern { pattern: r"^DIRECT DEBIT (?:XX)?\d+ (?:AUSTRALIAN FELLO|AFES)", canonical: "AFES (Donation)" },
    KnownMerchantPattern { pattern: r"^TO AFES", canonical: "AFES (Donation)" },
    KnownMerchantPattern { pattern: r"^TRANSFER TO AFES", canonical: "AFES (Donation)" },
    KnownMerchantPattern { pattern: r"^RETURN.*DIRECT DEBIT \d+ AFES", canonical: "AFES (Donation Return)" },
    KnownMerchantPattern { pattern: r"^DIRECT CREDIT (?:XX)?\d+ MCARE BENEFITS", canonical: "Medicare Benefits" },
    KnownMerchantPattern { pattern: r"^DIRECT CREDIT (?:XX)?\d+ ATO", canonical: "ATO" },
    KnownMerchantPattern { pattern: r"^DIRECT DEBIT (?:XX)?\d+ COMMINSURE", canonical: "CommInsure" },
    KnownMerchantPattern { pattern: r"^DIRECT CREDIT (?:PENSION )?\d+ CHILDASSISTPYMT", canonical: "Child Assist Payment" },
    KnownMerchantPattern { pattern: r"^DIRECT CREDIT (?:PENSION )?(?:XX)?\d+ CHILDASSISTPYMT", canonical: "Child Assist Payment" },
    KnownMerchantPattern { pattern: r"^DIRECT CREDIT PENSION (?:XX)?\d+", canonical: "Centrelink Pension" },
    KnownMerchantPattern { pattern: r"^DIRECT CREDIT (?:XX)?\d+ ECONOMIC SUPPORT", canonical: "Economic Support Payment" },
    KnownMerchantPattern { pattern: r"^DIRECT CREDIT (?:XX)?\d+ COST OF LIVING", canonical: "Cost of Living Payment" },
    KnownMerchantPattern { pattern: r"^DIRECT CREDIT (?:XX)?\d+ NDIS NSW", canonical: "NDIS Payment" },
    KnownMerchantPattern { pattern: r"^DIRECT CREDIT (?:XX)?\d+ HCF", canonical: "HCF Health" },
    KnownMerchantPattern { pattern: r"^DIRECT CREDIT (?:XX)?\d+ BUPA", canonical: "BUPA" },
    KnownMerchantPattern { pattern: r"^DIRECT DEBIT (?:XX)?\d+ AMERICAN EXPRESS", canonical: "American Express Payment" },
    KnownMerchantPattern { pattern: r"^DIRECT DEBIT (?:XX)?\d+ OMF", canonical: "OMF International" },
    KnownMerchantPattern { pattern: r"^DIRECT DEBIT (?:XX)?\d+ PIONEERS", canonical: "Pioneers of Australia" },
    KnownMerchantPattern { pattern: r"^DIRECT DEBIT (?:XX)?\d+ HOLLARD", canonical: "Hollard Insurance" },
    KnownMerchantPattern { pattern: r"^DIRECT DEBIT (?:XX)?\d+ HUBHELLO", canonical: "Hubhello" },
    KnownMerchantPattern { pattern: r"^DIRECT DEBIT (?:XX)?\d+ AUSTRALIAN FELLO", canonical: "AFES (Donation)" },
    KnownMerchantPattern { pattern: r"^DIRECT CREDIT (?:XX)?\d+ FMC", canonical: "FMC" },
    KnownMerchantPattern { pattern: r"^(?:PAYMENT|TRANSFER) FROM TAM S", canonical: "Joint Account (Tam)" },
    KnownMerchantPattern { pattern: r"^(?:BPAY|BPAY) PAYMENT", canonical: "BPAY Payment" },
    KnownMerchantPattern { pattern: r"^ONLINE PAYMENT RECEIVED", canonical: "Online Payment Received" },
    KnownMerchantPattern { pattern: r"^INTERNAL TRANSFER", canonical: "Internal Account Transfer" },
    KnownMerchantPattern { pattern: r"^IFTTT STRAVA ACTIVITY", canonical: "IFTTT Strava" },
    KnownMerchantPattern { pattern: r"^INTERNATIONAL TRANSACTION FEE", canonical: "International Transaction Fee" },
    KnownMerchantPattern { pattern: r"^REPAYMENT/PAYMENT", canonical: "Repayment/Payment" },
    KnownMerchantPattern { pattern: r"^STRATHFIELD COUNCIL", canonical: "Strathfield Council" },
    KnownMerchantPattern { pattern: r"^SYDNEY WATER", canonical: "Sydney Water" },
    KnownMerchantPattern { pattern: r"^ENERGYAUSTRALIA", canonical: "EnergyAustralia" },
    KnownMerchantPattern { pattern: r"^TRANSFER TO OTHER BANK", canonical: "Transfer to Other Bank" },
    KnownMerchantPattern { pattern: r"^TRANSFER TO CBA", canonical: "Transfer to CBA" },
    KnownMerchantPattern { pattern: r"^TRANSFER (?:TO|FROM) XX\d{4}\b", canonical: "Internal Account Transfer" },
    KnownMerchantPattern { pattern: r"^FROM ACCOUNT XX\d{4}$", canonical: "Internal Account Transfer" },
    KnownMerchantPattern { pattern: r"^TO ACCOUNT XX\d{4}$", canonical: "Internal Account Transfer" },
    // Merchant chains
    KnownMerchantPattern { pattern: r"^WOOLWORTHS METRO\b", canonical: "Woolworths Metro" },
    KnownMerchantPattern { pattern: r"^WOOLWORTHS\b", canonical: "Woolworths" },
    KnownMerchantPattern { pattern: r"^COLES EXPRESS\b", canonical: "Coles Express" },
    KnownMerchantPattern { pattern: r"^COLES\b", canonical: "Coles" },
    KnownMerchantPattern { pattern: r"^MCDONALD'S\b", canonical: "McDonald's" },
    KnownMerchantPattern { pattern: r"^HERO SUSHI\b", canonical: "Hero Sushi" },
    KnownMerchantPattern { pattern: r"^STARBUCKS\b", canonical: "Starbucks" },
    KnownMerchantPattern { pattern: r"^CHEMIST WAREHOUSE\b", canonical: "Chemist Warehouse" },
    KnownMerchantPattern { pattern: r"^7-ELEVEN\b", canonical: "7-Eleven" },
    KnownMerchantPattern { pattern: r"^KFC\b", canonical: "KFC" },
    KnownMerchantPattern { pattern: r"^ALDI\b", canonical: "ALDI" },
    KnownMerchantPattern { pattern: r"^BWS\b", canonical: "BWS" },
    KnownMerchantPattern { pattern: r"^EG FUELCO\b", canonical: "EG Fuelco" },
    KnownMerchantPattern { pattern: r"^BAKERS DELIGHT\b", canonical: "Bakers Delight" },
    KnownMerchantPattern { pattern: r"^GONG CHA\b", canonical: "Gong Cha" },
    KnownMerchantPattern { pattern: r"^KMART\b", canonical: "Kmart" },
    KnownMerchantPattern { pattern: r"^OFFICEWORKS\b", canonical: "Officeworks" },
    KnownMerchantPattern { pattern: r"^BUNNINGS\b", canonical: "Bunnings" },
    KnownMerchantPattern { pattern: r"^TARGET\b", canonical: "Target" },
    KnownMerchantPattern { pattern: r"^DAISO\b", canonical: "Daiso" },
    KnownMerchantPattern { pattern: r"^DAVID JONES LIMITED", canonical: "David Jones Online" },
    KnownMerchantPattern { pattern: r"^DAVID JONES\b", canonical: "David Jones" },
    KnownMerchantPattern { pattern: r"^MACHI\b", canonical: "Machi" },
    KnownMerchantPattern { pattern: r"^SERVICE NSW\b", canonical: "Service NSW" },
    KnownMerchantPattern { pattern: r"^HAN SANG\b", canonical: "Han Sang" },
    KnownMerchantPattern { pattern: r"^VN CITY\b", canonical: "VN City" },
    KnownMerchantPattern { pattern: r"^ATM 210 BURWOOD [A-Z]$", canonical: "ATM Burwood" },
    KnownMerchantPattern { pattern: r"^ATM\b", canonical: "ATM" },
    // Specific merchants
    KnownMerchantPattern { pattern: r"^HERITAGE COFFEE SUMMER", canonical: "Heritage Coffee Summer Hill" },
    KnownMerchantPattern { pattern: r"^LUNEBURGER (?:AUSTRALIA )?(?:C|CENT|QVB|QV).*(?:SYDNEY|DARLINGHURST)", canonical: "Luneburger Sydney" },
    KnownMerchantPattern { pattern: r"^LUNEBURGER GERMAN BAKER", canonical: "Luneburger North Ryde" },
    KnownMerchantPattern { pattern: r"^CAFE SIENNA.*BURWOOD", canonical: "Cafe Sienna Burwood" },
    KnownMerchantPattern { pattern: r"^GOOD VEN(?:TURE|TRE).*STRATHFIELD", canonical: "Good Venture Partners Strathfield" },
    KnownMerchantPattern { pattern: r"^AUVERS (?:CAFE|INGENS)", canonical: "Auvers Cafe" },
    KnownMerchantPattern { pattern: r"^HONG KONG BING SUTT.*BURWOOD", canonical: "Hong Kong Bing Sutt Burwood" },
    KnownMerchantPattern { pattern: r"^PLINE PH (.+)", canonical: "Priceline Pharmacy" },
    KnownMerchantPattern { pattern: r"^DOPA DONBURI.*MARTIN", canonical: "Dopa Donburi Martin Place" },
    KnownMerchantPattern { pattern: r"^DOPA MARTIN", canonical: "Dopa Martin Place" },
    KnownMerchantPattern { pattern: r"^DOPA DONBURI.*MILKBA", canonical: "Dopa North Ryde" },
    KnownMerchantPattern { pattern: r"^DOPA BY DEVON", canonical: "Dopa by Devon Sydney" },
    KnownMerchantPattern { pattern: r"^DOPA SYDNEY$", canonical: "Dopa Sydney" },
    KnownMerchantPattern { pattern: r"^DOPA\b", canonical: "Dopa" },
    KnownMerchantPattern { pattern: r"^MEMOCORP AUSTRALIA", canonical: "Memocorp Australia Strathfield" },
    KnownMerchantPattern { pattern: r"^THE AVENUE.*NEWINGTON", canonical: "The Avenue Newington" },
    KnownMerchantPattern { pattern: r"^LINKT SYDNEY", canonical: "Linkt" },
    KnownMerchantPattern { pattern: r"^LINKT\b", canonical: "Linkt" },
    KnownMerchantPattern { pattern: r"^GUZMAN Y GOMEZ.*GYG", canonical: "Guzman Y Gomez" },
    KnownMerchantPattern { pattern: r"^GUZMAN Y GOMEZ MARTIN", canonical: "Guzman Y Gomez Sydney" },
    KnownMerchantPattern { pattern: r"^GUZMAN Y GOMEZ\b", canonical: "Guzman Y Gomez" },
    KnownMerchantPattern { pattern: r"^SUSHI HUB QVB", canonical: "Sushi Hub QVB Sydney" },
    KnownMerchantPattern { pattern: r"^SUSHI NAYA WESTFIELD", canonical: "Sushi Naya Burwood" },
    KnownMerchantPattern { pattern: r"^SUSHI NAYA MARTIN", canonical: "Sushi Naya Sydney" },
    KnownMerchantPattern { pattern: r"^SUSHI NAYA\b", canonical: "Sushi Naya" },
    KnownMerchantPattern { pattern: r"^SUSHI WORLD\b", canonical: "Sushi World" },
    KnownMerchantPattern { pattern: r"^POP MART OCEANIA", canonical: "Pop Mart" },
    KnownMerchantPattern { pattern: r"^POP MART\b", canonical: "Pop Mart" },
    KnownMerchantPattern { pattern: r"^DOROTHY COWIE SCHOOL", canonical: "Dorothy Cowie School" },
    KnownMerchantPattern { pattern: r"^EAT ISTANBUL MARTIN", canonical: "Eat Istanbul Sydney" },
    KnownMerchantPattern { pattern: r"^EAT ISTANBUL\b", canonical: "Eat Istanbul" },
    KnownMerchantPattern { pattern: r"^IVY.*MUMU", canonical: "Ivy Mumu Sydney" },
    KnownMerchantPattern { pattern: r"^TEA SPOT.*BURWOOD", canonical: "Tea Spot Burwood" },
    KnownMerchantPattern { pattern: r"^LEIBLE COFFEE", canonical: "Leible Coffee Sydney" },
    KnownMerchantPattern { pattern: r"^PRICELINE PHARMACY WYNY", canonical: "Priceline Pharmacy Sydney" },
    KnownMerchantPattern { pattern: r"^PRICELINE PHARMACY\b", canonical: "Priceline Pharmacy" },
    KnownMerchantPattern { pattern: r"^MACQUARIE UNIVERSITY", canonical: "Macquarie University" },
    KnownMerchantPattern { pattern: r"^MEET FRESH.*BURWOOD", canonical: "Meet Fresh Burwood" },
    KnownMerchantPattern { pattern: r"^MEET FRESH\b", canonical: "Meet Fresh" },
    KnownMerchantPattern { pattern: r"^MAENAM LAO THAI.*STRATHFIELD", canonical: "Maenam Lao Thai Strathfield" },
    KnownMerchantPattern { pattern: r"^MAENAM LAO.*MELBOURNE", canonical: "Maenam Lao Melbourne" },
    KnownMerchantPattern { pattern: r"^YAKITORI JIN$", canonical: "Yakitori Jin" },
    KnownMerchantPattern { pattern: r"^CORNERSTONE CONCORD", canonical: "Cornerstone Concord" },
    KnownMerchantPattern { pattern: r"^KMART BURWOOD \d+", canonical: "Kmart Burwood" },
    KnownMerchantPattern { pattern: r"^INTERPARK UTS", canonical: "Interpark UTS Ultimo" },
    KnownMerchantPattern { pattern: r"^HEALTHY CARE BURWOOD", canonical: "Healthy Care Burwood" },
    KnownMerchantPattern { pattern: r"^GREENWAY MEAT AND FD", canonical: "Greenway Meat Strathfield" },
    KnownMerchantPattern { pattern: r"^GREENWAY MEAT\b", canonical: "Greenway Meat Strathfield" },
    KnownMerchantPattern { pattern: r"^NEWYEN INVESTMENT", canonical: "Newyen Investment Crows Nest" },
    KnownMerchantPattern { pattern: r"^AMOREPACIFIC AUSTRAL", canonical: "Amorepacific Burwood" },
    KnownMerchantPattern { pattern: r"^VISCO UNIVERSAL", canonical: "Visco Universal" },
    KnownMerchantPattern { pattern: r"^JFC STRATHFIELD", canonical: "JFC Strathfield" },
    KnownMerchantPattern { pattern: r"^KOMART(?:\s+NORTH\s+STRAT)?$", canonical: "Komart North Strathfield" },
    KnownMerchantPattern { pattern: r"^CONTRIBUTION TAX ADJUSTMENT$", canonical: "Contribution Tax" },
    KnownMerchantPattern { pattern: r"^DAISO BURWOOD", canonical: "Daiso Burwood" },
    KnownMerchantPattern { pattern: r"^MINISO BURWOOD", canonical: "Miniso Burwood" },
    KnownMerchantPattern { pattern: r"^AMZNPRIMEAU MEMBERSHIP", canonical: "Amazon Prime" },
    KnownMerchantPattern { pattern: r"^ANZ INTERNET BANKING BPAY", canonical: "ANZ BPAY" },
    KnownMerchantPattern { pattern: r"^FLOWER POWER.*ENFIELD", canonical: "Flower Power Enfield" },
    KnownMerchantPattern { pattern: r"^FLOWER POWER\b", canonical: "Flower Power" },
    KnownMerchantPattern { pattern: r"^OFFICEWORKS NORTH RYDE", canonical: "Officeworks North Ryde" },
    KnownMerchantPattern { pattern: r"^BP ENFIELD", canonical: "BP Enfield" },
    KnownMerchantPattern { pattern: r"^AUSKO COOPERATION", canonical: "Ausko Cooperation Concord" },
    KnownMerchantPattern { pattern: r"^GNT SERVICES$", canonical: "GNT Services North Strathfield" },
    KnownMerchantPattern { pattern: r"^CAMPION EDUCATION$", canonical: "Campion Education" },
    KnownMerchantPattern { pattern: r"^MANCINI S PIZZERIA", canonical: "Mancini's Pizzeria Belfield" },
    KnownMerchantPattern { pattern: r"^MANCINIS WOOD MELBOURNE", canonical: "Mancinis Wood Melbourne" },
    KnownMerchantPattern { pattern: r"^OMF(?:\s+INTERNATIONAL)?$", canonical: "OMF International" },
    KnownMerchantPattern { pattern: r"^HOLLARD(?:INS|\s+INSURANCE)", canonical: "Hollard Insurance" },
    KnownMerchantPattern { pattern: r"^PIONEERS OF(?:\s+AUST(?:RALIA)?)?$", canonical: "Pioneers of Australia" },
    KnownMerchantPattern { pattern: r"^FMC$", canonical: "FMC" },
    KnownMerchantPattern { pattern: r"^PAPPARICH MACQUARIE", canonical: "Papparich Macquarie Park" },
    KnownMerchantPattern { pattern: r"^PAPPARICH\b", canonical: "Papparich" },
    KnownMerchantPattern { pattern: r"^JASON HUI?$", canonical: "Jason Hui" },
    KnownMerchantPattern { pattern: r"^AIRPORT RETAIL ENTER", canonical: "Airport Retail Enterprises" },
    KnownMerchantPattern { pattern: r"^BURWOOD DISCOUNT CHEMIST", canonical: "Burwood Discount Chemist Burwood" },
    KnownMerchantPattern { pattern: r"^PAYPAL \*BUDGETPETPR", canonical: "PayPal Budget Pet Products" },
    KnownMerchantPattern { pattern: r"^CASH DEPOSIT.*BEEM IT", canonical: "Cash Deposit Beem It" },
    KnownMerchantPattern { pattern: r"^AES$", canonical: "AES Electrical" },
    KnownMerchantPattern { pattern: r"^TAN HANDS", canonical: "Tan Hands Physiotherapy" },
    KnownMerchantPattern { pattern: r"^GENESIS GARDENS", canonical: "Genesis Gardens" },
    KnownMerchantPattern { pattern: r"^OILSTONE", canonical: "Oilstone" },
    KnownMerchantPattern { pattern: r"^ING$", canonical: "ING" },
    KnownMerchantPattern { pattern: r"^CS EDUCATION", canonical: "CS Education Strathfield" },
    KnownMerchantPattern { pattern: r"^CS$", canonical: "CS Education Strathfield" },
    KnownMerchantPattern { pattern: r"^EARLY PURPOSE", canonical: "Early Purpose" },
    KnownMerchantPattern { pattern: r"^BAKED BEATS", canonical: "Baked Beats" },
    KnownMerchantPattern { pattern: r"^SLOWER DEEPER WISER", canonical: "Slower Deeper Wiser" },
    KnownMerchantPattern { pattern: r"^SOULGRAM PARTNERS", canonical: "Soulgram Partners" },
];

fn compiled_merchants() -> &'static [CompiledMerchant] {
    static COMPILED: OnceLock<Vec<CompiledMerchant>> = OnceLock::new();
    COMPILED.get_or_init(|| {
        KNOWN_MERCHANT_PATTERNS
            .iter()
            .map(|mp| CompiledMerchant {
                regex: Regex::new(mp.pattern).expect("invalid merchant pattern"),
                canonical: mp.canonical,
            })
            .collect()
    })
}

/// Match the uppercase stripped payee against known merchant patterns.
pub fn extract_merchant(stripped: &str, _original: &str) -> Option<String> {
    let upper = stripped.to_uppercase();
    for m in compiled_merchants() {
        if m.regex.is_match(&upper) {
            return Some(m.canonical.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_merchant_woolworths() {
        let result = extract_merchant("WOOLWORTHS 1624 STRATHFIELD", "WOOLWORTHS 1624 STRATHFIELD");
        assert_eq!(result, Some("Woolworths".to_string()));
    }
}

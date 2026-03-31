/// Known entity lookup tables compiled into the binary.
/// Seeded from experiment/normalise-rust rules YAML files.

/// A pattern-to-canonical-name mapping for entity lookup.
pub struct KnownEntity {
    pub pattern: &'static str,
    pub canonical: &'static str,
}

pub struct KnownMerchantPattern {
    pub pattern: &'static str,
    pub canonical: &'static str,
}

pub const KNOWN_MERCHANT_PATTERNS: &[KnownMerchantPattern] = &[
    // Banking identity mappings (higher priority — matched first)
    KnownMerchantPattern { pattern: r"^DIRECT DEBIT (?:XX)?\d+ (?:AUSTRALIAN FELLO|AFES)", canonical: "AFES (Donation)" },
    KnownMerchantPattern { pattern: r"^TO AFES", canonical: "AFES (Donation)" },
    KnownMerchantPattern { pattern: r"^TRANSFER TO AFES", canonical: "AFES (Donation)" },
    KnownMerchantPattern { pattern: r"^RETURN.*DIRECT DEBIT \d+ AFES", canonical: "AFES (Donation Return)" },
    KnownMerchantPattern { pattern: r"^DIRECT CREDIT (?:XX)?\d+ AFES", canonical: "AFES (Sophia Salary)" },
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
    KnownMerchantPattern { pattern: r"^GHOST LOCOMOTION AUSTRALIA", canonical: "Ghost Locomotion" },
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

// --- Known Persons ---
pub const KNOWN_PERSONS: &[KnownEntity] = &[
    KnownEntity { pattern: "LOK SUN NELSON TAM", canonical: "Nelson Tam" },
    KnownEntity { pattern: "LOK SUN NELSON T", canonical: "Nelson Tam" },
    KnownEntity { pattern: "NELSON TAM", canonical: "Nelson Tam" },
    KnownEntity { pattern: "TAM LOK SUN NELSON", canonical: "Nelson Tam" },
    KnownEntity { pattern: "LOK SUN TAM", canonical: "Nelson Tam" },
    KnownEntity { pattern: "SOPHIA S TAM", canonical: "Sophia Tam" },
    KnownEntity { pattern: "SOPHIA TAM", canonical: "Sophia Tam" },
    KnownEntity { pattern: "TAM S S & TAM L N", canonical: "Joint Account (Tam)" },
    KnownEntity { pattern: "TAM S S & TAM L", canonical: "Joint Account (Tam)" },
    KnownEntity { pattern: "TAM S & TAM L N", canonical: "Joint Account (Tam)" },
    KnownEntity { pattern: "TAM S S TAM L N", canonical: "Joint Account (Tam)" },
    KnownEntity { pattern: "TAM S S TAM L", canonical: "Joint Account (Tam)" },
    KnownEntity { pattern: "TAM S &", canonical: "Joint Account (Tam)" },
    KnownEntity { pattern: "TAM S TAM L N", canonical: "Joint Account (Tam)" },
    KnownEntity { pattern: "TAM S TAM L", canonical: "Joint Account (Tam)" },
    KnownEntity { pattern: "MR DAVID JAMES GREENAWAY", canonical: "David Greenaway" },
    KnownEntity { pattern: "MR DAVID JAMES GREENA", canonical: "David Greenaway" },
    KnownEntity { pattern: "DAVID GREENAWAY", canonical: "David Greenaway" },
    KnownEntity { pattern: "CORNERSTONE PRESBYTERIAN CHU", canonical: "Cornerstone Presbyterian Church" },
    KnownEntity { pattern: "CORNERSTONE PRESBYTERIAN COM", canonical: "Cornerstone Presbyterian Church" },
    KnownEntity { pattern: "ALEXANDER BAHN", canonical: "Alex Bahn" },
    KnownEntity { pattern: "BRIDONIE", canonical: "Bridonie Nicholson" },
    KnownEntity { pattern: "CHENHAN", canonical: "Chenhan Ma" },
    KnownEntity { pattern: "DENNIS", canonical: "Dennis Law" },
    KnownEntity { pattern: "GILLIAN", canonical: "Gillian Li" },
    KnownEntity { pattern: "HANNAH T", canonical: "Hannah Tarrant" },
    KnownEntity { pattern: "HOERMANN S R", canonical: "Stephan Hoermann" },
    KnownEntity { pattern: "JANETTE", canonical: "Janette Vardy" },
    KnownEntity { pattern: "JASON HU", canonical: "Jason Hui" },
    KnownEntity { pattern: "JOHNNY CHUNG LEUNG T", canonical: "Johnny Tam" },
    KnownEntity { pattern: "JOHNNY", canonical: "Johnny Tam" },
    KnownEntity { pattern: "NINGJIA AND SHAWN", canonical: "Ningjia Wang" },
    KnownEntity { pattern: "NINGJIA", canonical: "Ningjia Wang" },
    KnownEntity { pattern: "VONNIE HO CHING YEE", canonical: "Vonnie Ho Ching Yee" },
    KnownEntity { pattern: "VONNIE HO", canonical: "Vonnie Ho Ching Yee" },
    KnownEntity { pattern: "VONNIE", canonical: "Vonnie Ho Ching Yee" },
    KnownEntity { pattern: "YONNIE", canonical: "Yonnie Ho" },
    KnownEntity { pattern: "ZOE", canonical: "Zoe Fan" },
    KnownEntity { pattern: "STEFAN", canonical: "Stefan Gotz" },
    KnownEntity { pattern: "HSIN YEN CINDY TAN", canonical: "Cindy Tan" },
    KnownEntity { pattern: "HSIN TAN", canonical: "Cindy Tan" },
    KnownEntity { pattern: "SAMULE LI SAM", canonical: "Samule Li" },
    KnownEntity { pattern: "SAMULE", canonical: "Samule Li" },
    KnownEntity { pattern: "ANNA MCQUEEN", canonical: "Anna McQueen" },
    KnownEntity { pattern: "ANNA", canonical: "Anna McQueen" },
    KnownEntity { pattern: "JEN", canonical: "Jen Tan" },
    KnownEntity { pattern: "CHARLOTTE", canonical: "Charlotte Hitchcock" },
    KnownEntity { pattern: "REBECCA", canonical: "Rebecca Ng" },
    KnownEntity { pattern: "RICHARD", canonical: "Richard Ho" },
    KnownEntity { pattern: "G W WONG SEE", canonical: "Graeme Wong See" },
    KnownEntity { pattern: "GW WONG SEE", canonical: "Graeme Wong See" },
    KnownEntity { pattern: "LAM HARRY", canonical: "Harry Lam" },
    KnownEntity { pattern: "DAVIES K", canonical: "Kirsten Davies" },
    KnownEntity { pattern: "TIMMS LANA GRACE", canonical: "Lana Grace Timms" },
    KnownEntity { pattern: "LANG KENNETH HAMES", canonical: "Lang Hames" },
    KnownEntity { pattern: "THOMAS MM LEONG", canonical: "Thomas Leong" },
    KnownEntity { pattern: "ELDER ROBERT GRAHAME", canonical: "Rob Elder" },
    KnownEntity { pattern: "ADAM MCCANN", canonical: "Adam McCann" },
    KnownEntity { pattern: "SIMON AND STEPHANIE WONG", canonical: "Stephanie and Simon Wong" },
    KnownEntity { pattern: "EMILY AND JASON", canonical: "Emily and Jason Hui" },
    KnownEntity { pattern: "GRACE AND HANNAH", canonical: "Grace and Hannah Chan" },
    KnownEntity { pattern: "SAM AND KIRBY ATWOOD", canonical: "Samuel Atwood" },
    KnownEntity { pattern: "MISS SHARON LAW", canonical: "Sharon Law" },
    KnownEntity { pattern: "MISS NERIDA GIFFORD", canonical: "Nerida Gifford" },
    KnownEntity { pattern: "MISS TZVETELINA PETKOVA", canonical: "Tzvetelina Petkova" },
    KnownEntity { pattern: "MR ANDY CHI-KIT TAN", canonical: "Andy Tan" },
    KnownEntity { pattern: "MR CHRISTOPHER CHUN KIT WONG", canonical: "Christopher Wong" },
    KnownEntity { pattern: "MR HOCK LIM OOI", canonical: "Hock Ooi" },
    KnownEntity { pattern: "MR PHILIP ANDREW SNEL", canonical: "Philip Snelling" },
    KnownEntity { pattern: "MR PIERRE JEAN-LUC TH", canonical: "Pierre Thielemans" },
    KnownEntity { pattern: "MR TRISTAN ALEXANDER", canonical: "Tristan McBide" },
    KnownEntity { pattern: "MR VINH GIA TRAN", canonical: "Vinh Tran" },
    KnownEntity { pattern: "MR SAMUEL DAVID ATWOO", canonical: "Samuel Atwood" },
    KnownEntity { pattern: "MS DORIS MING WAI CHO", canonical: "Doris Chong" },
    KnownEntity { pattern: "MS MAREE JAYASHREE MO", canonical: "Maree Selvaraj" },
    KnownEntity { pattern: "MS HOI WAN LI & MR KI", canonical: "Hoi Wan Li and Jason Tam" },
    KnownEntity { pattern: "MS NATASHA LO", canonical: "Natasha Lo" },
    KnownEntity { pattern: "MRS HEA-WON PARK", canonical: "Mrs Hea-Won Park" },
    KnownEntity { pattern: "STEPHANIE WONG", canonical: "Stephanie Wong" },
    KnownEntity { pattern: "STEPHAN HOERMANN", canonical: "Stephan Hoermann" },
    KnownEntity { pattern: "SAIDGANI MUSAEV", canonical: "Saidgani Musaev" },
    KnownEntity { pattern: "MARTIN HIGHLAND", canonical: "Martin Highland" },
    KnownEntity { pattern: "A S-W BYWATERS", canonical: "A S-W Bywaters" },
    KnownEntity { pattern: "A VENTURA MENDOZA", canonical: "A Ventura Mendoza" },
    KnownEntity { pattern: "ADAM RASKO", canonical: "Adam Rasko" },
    KnownEntity { pattern: "BETHANY MACEY", canonical: "Bethany Macey" },
    KnownEntity { pattern: "BIANCA SUNITO", canonical: "Bianca Sunito" },
    KnownEntity { pattern: "CHARLOTTE FIELD", canonical: "Charlotte Field" },
    KnownEntity { pattern: "CHESTER WONG", canonical: "Chester Wong" },
    KnownEntity { pattern: "ELIJAH MUCCI", canonical: "Elijah Mucci" },
    KnownEntity { pattern: "ELSPETH MEEK", canonical: "Elspeth Meek" },
    KnownEntity { pattern: "ETHAN LUM MOW", canonical: "Ethan Lum Mow" },
    KnownEntity { pattern: "HANNAH TARRANT", canonical: "Hannah Tarrant" },
    KnownEntity { pattern: "JAMES MULHOLLAND", canonical: "James Mulholland" },
    KnownEntity { pattern: "JANETTE VARDY", canonical: "Janette Vardy" },
    KnownEntity { pattern: "JESSICA PIREH", canonical: "Jessica Pireh" },
    KnownEntity { pattern: "MANISH SENEVIRATNE", canonical: "Manish Seneviratne" },
    KnownEntity { pattern: "MISS ELIZABETH JOY TH", canonical: "Elizabeth Joy" },
    KnownEntity { pattern: "MISS EMILY MAREE BENN", canonical: "Emily Maree Benn" },
    KnownEntity { pattern: "MISS TABITHA WOOD", canonical: "Tabitha Wood" },
    KnownEntity { pattern: "MR CALEB ANDREW MITCH", canonical: "Caleb Mitchell" },
    KnownEntity { pattern: "MR THOMAS ROSS BAKER", canonical: "Thomas Ross Baker" },
    KnownEntity { pattern: "NATALIA TABERNER", canonical: "Natalia Taberner" },
    KnownEntity { pattern: "NERIDA GIFFORD", canonical: "Nerida Gifford" },
    KnownEntity { pattern: "ROGER CHAN", canonical: "Roger Chan" },
    KnownEntity { pattern: "SAMULE LI", canonical: "Samule Li" },
    KnownEntity { pattern: "SERENA E CHIERT", canonical: "Serena E Chiert" },
    KnownEntity { pattern: "ZACHARY MATTHEW FISHE", canonical: "Zachary Fisher" },
    KnownEntity { pattern: "KOBE CC TSANG", canonical: "Kobe Cc Tsang" },
    KnownEntity { pattern: "SHARON LAW", canonical: "Sharon Law" },
    KnownEntity { pattern: "MR", canonical: "Unknown Person" },
    KnownEntity { pattern: "MISS", canonical: "Unknown Person" },
    KnownEntity { pattern: "MRS", canonical: "Unknown Person" },
    KnownEntity { pattern: "TAM", canonical: "Nelson Tam" },
    KnownEntity { pattern: "LOK", canonical: "Nelson Tam" },
];

pub const PERSONS_STRIP_MEMO: &[&str] = &[
    "JOHNNY TAM", "DAVID GREENAWAY", "MR DAVID JAMES GREENAWAY",
    "MRS STEPHANIE WONG", "STEPHAN HOERMANN", "LOK SUN NELSON TAM",
    "SAIDGANI MUSAEV", "TAM S & TAM L N", "TAM S TAM L N",
    "TAM S TAM L", "TAM S S TAM L N", "TAM S S TAM L",
    "A S-W BYWATERS", "A VENTURA MENDOZA", "ADAM RASKO",
    "BETHANY MACEY", "BIANCA SUNITO", "CHARLOTTE FIELD",
    "CHESTER WONG", "ELIJAH MUCCI", "ELSPETH MEEK",
    "ETHAN LUM MOW", "HANNAH TARRANT", "JAMES MULHOLLAND",
    "JANETTE VARDY", "JESSICA PIREH", "MANISH SENEVIRATNE",
    "MISS ELIZABETH JOY TH", "MISS EMILY MAREE BENN",
    "MISS TABITHA WOOD", "MR CALEB ANDREW MITCH",
    "MR THOMAS ROSS BAKER", "NATALIA TABERNER", "NERIDA GIFFORD",
    "ROGER CHAN", "SAMULE LI", "SERENA E CHIERT",
    "STEPHANIE WONG", "ZACHARY MATTHEW FISHE", "MARTIN HIGHLAND",
];

// --- Known Locations ---
pub const KNOWN_LOCATIONS: &[&str] = &[
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

// --- Known Banking Operations ---
pub const KNOWN_BANKING_OPS: &[KnownEntity] = &[
    KnownEntity { pattern: "INTEREST CHARGE", canonical: "Interest Charged" },
    KnownEntity { pattern: "INTEREST ADJUSTMENT", canonical: "Interest Adjustment" },
    KnownEntity { pattern: "INTEREST CORRECTION", canonical: "Interest Correction" },
    KnownEntity { pattern: "CREDIT CARD", canonical: "Credit Card" },
    KnownEntity { pattern: "FUNDS TRANSFER", canonical: "Funds Transfer" },
    KnownEntity { pattern: "ACCOUNT SERVICING FEE", canonical: "Account Servicing Fee" },
    KnownEntity { pattern: "GOVERNMENT SEARCH FEE", canonical: "Government Search Fee" },
    KnownEntity { pattern: "LOAN REPAYMENT", canonical: "Loan Repayment" },
    KnownEntity { pattern: "CASH DEPOSIT", canonical: "Cash Deposit" },
    KnownEntity { pattern: "WDL ATM", canonical: "ATM Withdrawal" },
];

// --- Employer patterns ---
pub struct EmployerPattern {
    pub pattern: &'static str,
    pub canonical: &'static str,
}

pub const KNOWN_EMPLOYERS: &[EmployerPattern] = &[
    EmployerPattern { pattern: "APPLE PTY LTD", canonical: "Apple" },
    EmployerPattern { pattern: "APPLE COMPUTERS", canonical: "Apple" },
    EmployerPattern { pattern: "APPLE COMPUTER AUSTRALIA PTY LTD", canonical: "Apple" },
    EmployerPattern { pattern: "AFES", canonical: "AFES" },
    EmployerPattern { pattern: "AUSTRALIAN FELLO", canonical: "AFES" },
];

// --- Cleanup constants ---
pub const UPPERCASE_EXCEPTIONS: &[&str] = &[
    "ATM", "NSW", "AUS", "AU", "UTS", "KFC", "CBD", "QVB", "BWS", "BHP",
    "ANZ", "CBA", "NAB", "NDIS", "HCF", "BUPA", "AFES", "AMEB", "ALDI",
    "IKEA", "MYER", "ZARA", "ASOS", "LEGO", "UFC", "ATO", "ABN", "ACN",
    "PTY", "LTD", "USA", "UK", "NZ", "JP", "ID", "SG", "PS", "TFNSW",
    "UBER", "PEXA", "VIC", "QLD", "WA", "SA", "TAS", "ACT", "NT",
    "JFC", "GNT", "CS", "BP", "OMF", "FMC", "ING", "GYG",
];

pub const LOWERCASE_EXCEPTIONS: &[&str] = &[
    "the", "and", "of", "in", "at", "for", "to", "by", "on", "or", "a", "an",
    "pty", "ltd",
];

pub struct CleanupPattern {
    pub pattern: &'static str,
}

pub const TRAILING_NOISE_PATTERNS: &[CleanupPattern] = &[
    CleanupPattern { pattern: r"\s+NS$" },
    CleanupPattern { pattern: r"\s+AU$" },
    CleanupPattern { pattern: r"\s+AUS$" },
    CleanupPattern { pattern: r"\s+NSWAU$" },
    CleanupPattern { pattern: r"\s+XX\d+$" },
    CleanupPattern { pattern: r"\s+X\d+$" },
    CleanupPattern { pattern: r"\s+\d{6}$" },
    CleanupPattern { pattern: r"\s+\d{7,}$" },
    CleanupPattern { pattern: r"\s+VISA$" },
    CleanupPattern { pattern: r"\s+BANK$" },
    CleanupPattern { pattern: r"\s+DEBIT$" },
    CleanupPattern { pattern: r"\s+CREDIT$" },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_known_merchant_patterns_not_empty() {
        assert!(!KNOWN_MERCHANT_PATTERNS.is_empty());
    }

    #[test]
    fn test_merchant_patterns_are_valid_regex() {
        for mp in KNOWN_MERCHANT_PATTERNS {
            regex::Regex::new(mp.pattern)
                .unwrap_or_else(|e| panic!("invalid merchant pattern '{}': {}", mp.pattern, e));
        }
    }

    #[test]
    fn test_known_persons_not_empty() {
        assert!(!KNOWN_PERSONS.is_empty());
    }

    #[test]
    fn test_persons_strip_memo_not_empty() {
        assert!(!PERSONS_STRIP_MEMO.is_empty());
    }

    #[test]
    fn test_known_locations_not_empty() {
        assert!(!KNOWN_LOCATIONS.is_empty());
    }

    #[test]
    fn test_known_banking_ops_not_empty() {
        assert!(!KNOWN_BANKING_OPS.is_empty());
    }

    #[test]
    fn test_known_employers_not_empty() {
        assert!(!KNOWN_EMPLOYERS.is_empty());
    }

    #[test]
    fn test_trailing_noise_patterns_are_valid_regex() {
        for p in TRAILING_NOISE_PATTERNS {
            regex::Regex::new(p.pattern)
                .unwrap_or_else(|e| panic!("invalid trailing noise pattern '{}': {}", p.pattern, e));
        }
    }
}

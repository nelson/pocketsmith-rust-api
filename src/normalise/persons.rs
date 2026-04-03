struct KnownEntity {
    pattern: &'static str,
    canonical: &'static str,
}

const KNOWN_PERSONS: &[KnownEntity] = &[
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

const PERSONS_STRIP_MEMO: &[&str] = &[
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

/// Extract a known person from the stripped payee and original description.
///
/// Checks strip_memo patterns against original, then tries transfer entity
/// patterns (e.g. "From X, to PayID"), then direct/prefix match on stripped.
pub fn extract_person(stripped: &str, original: &str) -> Option<String> {
    let upper_original = original.to_uppercase();
    let upper_stripped = stripped.to_uppercase();

    // 1. Check strip_memo entries against original (these are names embedded in transfer descriptions)
    for &memo in PERSONS_STRIP_MEMO {
        if upper_original.contains(memo) {
            if let Some(canonical) = lookup_person(memo) {
                return Some(canonical.to_string());
            }
        }
    }

    // 2. Direct/prefix match on stripped payee
    for person in KNOWN_PERSONS {
        if upper_stripped == person.pattern || upper_stripped.starts_with(&format!("{} ", person.pattern)) {
            return Some(person.canonical.to_string());
        }
    }

    None
}

fn lookup_person(name: &str) -> Option<&'static str> {
    for person in KNOWN_PERSONS {
        if person.pattern == name {
            return Some(person.canonical);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_person_johnny_tam() {
        let result = extract_person(
            "JOHNNY TAM",
            "Fast Transfer From Johnny Tam, to PayID Phone",
        );
        assert_eq!(result, Some("Johnny Tam".to_string()));
    }
}

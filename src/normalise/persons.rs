use std::sync::OnceLock;

use regex::Regex;

use super::{NormalisationResult, PayeeClass};

struct Person {
    canonical: &'static str,
    patterns: &'static [&'static str],
}

struct CompiledPerson {
    regex: Regex,
    canonical: &'static str,
}

fn compiled_persons() -> &'static [CompiledPerson] {
    static COMPILED: OnceLock<Vec<CompiledPerson>> = OnceLock::new();
    COMPILED.get_or_init(|| {
        KNOWN_PERSONS
            .iter()
            .flat_map(|p| {
                p.patterns.iter().map(move |&pat| CompiledPerson {
                    regex: Regex::new(&format!(
                        r"(?i)\b{}(?:\b|\s|$)",
                        regex::escape(pat)
                    ))
                    .expect("invalid person pattern"),
                    canonical: p.canonical,
                })
            })
            .collect()
    })
}

pub fn apply(result: &mut NormalisationResult) {
    for cp in compiled_persons() {
        if cp.regex.is_match(&result.normalised) {
            result.features.entity_name = Some(cp.canonical.to_string());
            result.set_class(PayeeClass::Person);
            return;
        }
    }
}

const KNOWN_PERSONS: &[Person] = &[
    Person { canonical: "A S-W Bywaters", patterns: &["A S-W BYWATERS"] },
    Person { canonical: "A Ventura Mendoza", patterns: &["A VENTURA MENDOZA"] },
    Person { canonical: "Adam McCann", patterns: &["ADAM MCCANN"] },
    Person { canonical: "Adam Rasko", patterns: &["ADAM RASKO"] },
    Person { canonical: "Alex Bahn", patterns: &["ALEXANDER BAHN"] },
    Person { canonical: "Andy Tan", patterns: &["MR ANDY CHI-KIT TAN"] },
    Person { canonical: "Anna McQueen", patterns: &["ANNA MCQUEEN", "ANNA"] },
    Person { canonical: "Bethany Macey", patterns: &["BETHANY MACEY"] },
    Person { canonical: "Bianca Sunito", patterns: &["BIANCA SUNITO"] },
    Person { canonical: "Bridonie Nicholson", patterns: &["BRIDONIE"] },
    Person { canonical: "Caleb Mitchell", patterns: &["MR CALEB ANDREW MITCH"] },
    Person { canonical: "Charlotte Field", patterns: &["CHARLOTTE FIELD"] },
    Person { canonical: "Charlotte Hitchcock", patterns: &["CHARLOTTE"] },
    Person { canonical: "Chenhan Ma", patterns: &["CHENHAN"] },
    Person { canonical: "Chester Wong", patterns: &["CHESTER WONG"] },
    Person { canonical: "Christopher Wong", patterns: &["MR CHRISTOPHER CHUN KIT WONG"] },
    Person { canonical: "Cindy Tan", patterns: &["HSIN YEN CINDY TAN", "HSIN TAN"] },
    Person { canonical: "Cornerstone Presbyterian Church", patterns: &["CORNERSTONE PRESBYTERIAN CHU", "CORNERSTONE PRESBYTERIAN COM"] },
    Person { canonical: "David Greenaway", patterns: &["MR DAVID JAMES GREENAWAY", "MR DAVID JAMES GREENA", "DAVID GREENAWAY"] },
    Person { canonical: "Dennis Law", patterns: &["DENNIS"] },
    Person { canonical: "Doris Chong", patterns: &["MS DORIS MING WAI CHO"] },
    Person { canonical: "Elijah Mucci", patterns: &["ELIJAH MUCCI"] },
    Person { canonical: "Elizabeth Joy", patterns: &["MISS ELIZABETH JOY TH"] },
    Person { canonical: "Elspeth Meek", patterns: &["ELSPETH MEEK"] },
    Person { canonical: "Emily and Jason Hui", patterns: &["EMILY AND JASON"] },
    Person { canonical: "Emily Maree Benn", patterns: &["MISS EMILY MAREE BENN"] },
    Person { canonical: "Ethan Lum Mow", patterns: &["ETHAN LUM MOW"] },
    Person { canonical: "Gillian Li", patterns: &["GILLIAN"] },
    Person { canonical: "Grace and Hannah Chan", patterns: &["GRACE AND HANNAH"] },
    Person { canonical: "Graeme Wong See", patterns: &["G W WONG SEE", "GW WONG SEE"] },
    Person { canonical: "Hannah Tarrant", patterns: &["HANNAH TARRANT", "HANNAH T"] },
    Person { canonical: "Harry Lam", patterns: &["LAM HARRY"] },
    Person { canonical: "Hock Ooi", patterns: &["MR HOCK LIM OOI"] },
    Person { canonical: "Hoi Wan Li and Jason Tam", patterns: &["MS HOI WAN LI & MR KI"] },
    Person { canonical: "James Mulholland", patterns: &["JAMES MULHOLLAND"] },
    Person { canonical: "Janette Vardy", patterns: &["JANETTE VARDY", "JANETTE"] },
    Person { canonical: "Jason Hui", patterns: &["JASON HU", "JASON HUI"] },
    Person { canonical: "Jen Tan", patterns: &["JEN"] },
    Person { canonical: "Jessica Pireh", patterns: &["JESSICA PIREH"] },
    Person {
        canonical: "Johnny Tam",
        patterns: &["JOHNNY CHUNG LEUNG T", "JOHNNY TAM", "JOHNNY"],
    },
    Person {
        canonical: "Joint Account (Tam)",
        patterns: &[
            "TAM S S & TAM L N",
            "TAM S S & TAM L",
            "TAM S & TAM L N",
            "TAM S S TAM L N",
            "TAM S S TAM L",
            "TAM S &",
            "TAM S TAM L N",
            "TAM S TAM L",
        ],
    },
    Person { canonical: "Kirsten Davies", patterns: &["DAVIES K"] },
    Person { canonical: "Kobe Cc Tsang", patterns: &["KOBE CC TSANG"] },
    Person { canonical: "Lana Grace Timms", patterns: &["TIMMS LANA GRACE"] },
    Person { canonical: "Lang Hames", patterns: &["LANG KENNETH HAMES"] },
    Person { canonical: "Manish Seneviratne", patterns: &["MANISH SENEVIRATNE"] },
    Person { canonical: "Maree Selvaraj", patterns: &["MS MAREE JAYASHREE MO"] },
    Person { canonical: "Martin Highland", patterns: &["MARTIN HIGHLAND"] },
    Person { canonical: "Mrs Hea-Won Park", patterns: &["MRS HEA-WON PARK"] },
    Person { canonical: "Natalia Taberner", patterns: &["NATALIA TABERNER"] },
    Person { canonical: "Natasha Lo", patterns: &["MS NATASHA LO"] },
    Person {
        canonical: "Nelson Tam",
        patterns: &[
            "LOK SUN NELSON TAM",
            "TAM LOK SUN NELSON",
            "LOK SUN NELSON T",
            "LOK SUN TAM",
            "NELSON TAM",
        ],
    },
    Person { canonical: "Nerida Gifford", patterns: &["MISS NERIDA GIFFORD", "NERIDA GIFFORD"] },
    Person { canonical: "Ningjia Wang", patterns: &["NINGJIA AND SHAWN", "NINGJIA"] },
    Person { canonical: "Philip Snelling", patterns: &["MR PHILIP ANDREW SNEL"] },
    Person { canonical: "Pierre Thielemans", patterns: &["MR PIERRE JEAN-LUC TH"] },
    Person { canonical: "Rebecca Ng", patterns: &["REBECCA"] },
    Person { canonical: "Richard Ho", patterns: &["RICHARD"] },
    Person { canonical: "Rob Elder", patterns: &["ELDER ROBERT GRAHAME"] },
    Person { canonical: "Roger Chan", patterns: &["ROGER CHAN"] },
    Person { canonical: "Saidgani Musaev", patterns: &["SAIDGANI MUSAEV"] },
    Person { canonical: "Samuel Atwood", patterns: &["SAM AND KIRBY ATWOOD", "MR SAMUEL DAVID ATWOO"] },
    Person { canonical: "Samule Li", patterns: &["SAMULE LI SAM", "SAMULE LI", "SAMULE"] },
    Person { canonical: "Serena E Chiert", patterns: &["SERENA E CHIERT"] },
    Person { canonical: "Sharon Law", patterns: &["MISS SHARON LAW", "SHARON LAW"] },
    Person {
        canonical: "Sophia Tam",
        patterns: &["SOPHIA S TAM", "SOPHIA TAM"],
    },
    Person { canonical: "Stefan Gotz", patterns: &["STEFAN"] },
    Person { canonical: "Stephan Hoermann", patterns: &["STEPHAN HOERMANN", "HOERMANN S R"] },
    Person { canonical: "Stephanie and Simon Wong", patterns: &["SIMON AND STEPHANIE WONG"] },
    Person { canonical: "Stephanie Wong", patterns: &["MRS STEPHANIE WONG", "STEPHANIE WONG"] },
    Person { canonical: "Tabitha Wood", patterns: &["MISS TABITHA WOOD"] },
    Person { canonical: "Thomas Leong", patterns: &["THOMAS MM LEONG"] },
    Person { canonical: "Thomas Ross Baker", patterns: &["MR THOMAS ROSS BAKER"] },
    Person { canonical: "Tristan McBide", patterns: &["MR TRISTAN ALEXANDER"] },
    Person { canonical: "Tzvetelina Petkova", patterns: &["MISS TZVETELINA PETKOVA"] },
    Person { canonical: "Vinh Tran", patterns: &["MR VINH GIA TRAN"] },
    Person { canonical: "Vonnie Ho Ching Yee", patterns: &["VONNIE HO CHING YEE", "VONNIE HO", "VONNIE"] },
    Person { canonical: "Yonnie Ho", patterns: &["YONNIE"] },
    Person { canonical: "Zachary Fisher", patterns: &["ZACHARY MATTHEW FISHE"] },
    Person { canonical: "Zoe Fan", patterns: &["ZOE"] },
    // Generic title patterns — must be last (least specific)
    Person { canonical: "Unknown Person", patterns: &["MR", "MISS", "MRS"] },
    Person { canonical: "Nelson Tam", patterns: &["TAM", "LOK"] },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_person_johnny_tam() {
        let mut r = NormalisationResult::new("JOHNNY TAM");
        apply(&mut r);
        assert_eq!(r.features.entity_name.as_deref(), Some("Johnny Tam"));
        assert_eq!(r.class(), Some(&PayeeClass::Person));
    }

    #[test]
    fn test_person_with_prefix() {
        let mut r = NormalisationResult::new("TRANSFER FROM NELSON TAM");
        apply(&mut r);
        assert_eq!(r.features.entity_name.as_deref(), Some("Nelson Tam"));
        assert_eq!(r.class(), Some(&PayeeClass::Person));
    }

    #[test]
    fn test_person_no_match() {
        let mut r = NormalisationResult::new("WOOLWORTHS STRATHFIELD");
        apply(&mut r);
        assert!(r.features.entity_name.is_none());
        assert!(r.class().is_none());
    }
}

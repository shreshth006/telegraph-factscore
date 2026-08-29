//! ISO 3166-1 alpha-2 country codes, paired with their single-token names.
//!
//! Why this exists. A run of proper nouns is already indexed by its initials, so
//! `US` finds `United States` with no table at all (`score::add_aliases`). That
//! rule cannot reach a *single-word* country: no lexical operation turns
//! `Uruguay` into `UY`, `Germany` into `DE`, or `Japan` into `JP`. Miners write
//! both forms constantly.
//!
//! Before this table, a two-letter ALL-CAPS token simply abstained — it counted
//! neither for nor against the answer. That was safe but blunt in both
//! directions: a correct `UY` earned no credit (measured 0.9696 against 1.0000
//! for the spelled-out form), and a **wrong** `DE` against a ground truth of
//! Uruguay cost nothing at all. With the table, a code that matches is support
//! and a code that does not is a wrong entity like any other.
//!
//! This is general knowledge — a published standard, not a fact about any miner
//! or any hidden fixture — so it stays inside the legitimacy boundary (A4).
//!
//! **Limits, stated rather than hidden.** Only countries whose common English
//! name is one token are here; multi-word names (`United Kingdom` -> `GB`,
//! `South Korea` -> `KR`, `United Arab Emirates` -> `AE`) are not reachable from
//! a single token and still fall back to the initials rule, which yields `UK`,
//! `SK` and `UAE` rather than the ISO code. Subdivision codes (US states,
//! Canadian provinces) are not modelled at all.

use crate::bytes::hash_str;

/// `(alpha-2 code, single-token English name)`, as case-folded FNV-1a hashes.
/// Ordered by nothing in particular; the lookup is a linear scan over ~120
/// entries, which is far cheaper than the tokenisation that precedes it.
static CODES: [(u32, u32); 122] = [
    (hash_str("af"), hash_str("afghanistan")),
    (hash_str("al"), hash_str("albania")),
    (hash_str("dz"), hash_str("algeria")),
    (hash_str("ad"), hash_str("andorra")),
    (hash_str("ao"), hash_str("angola")),
    (hash_str("ar"), hash_str("argentina")),
    (hash_str("am"), hash_str("armenia")),
    (hash_str("au"), hash_str("australia")),
    (hash_str("at"), hash_str("austria")),
    (hash_str("az"), hash_str("azerbaijan")),
    (hash_str("bh"), hash_str("bahrain")),
    (hash_str("bd"), hash_str("bangladesh")),
    (hash_str("by"), hash_str("belarus")),
    (hash_str("be"), hash_str("belgium")),
    (hash_str("bz"), hash_str("belize")),
    (hash_str("bj"), hash_str("benin")),
    (hash_str("bt"), hash_str("bhutan")),
    (hash_str("bo"), hash_str("bolivia")),
    (hash_str("bw"), hash_str("botswana")),
    (hash_str("br"), hash_str("brazil")),
    (hash_str("bg"), hash_str("bulgaria")),
    (hash_str("kh"), hash_str("cambodia")),
    (hash_str("cm"), hash_str("cameroon")),
    (hash_str("ca"), hash_str("canada")),
    (hash_str("td"), hash_str("chad")),
    (hash_str("cl"), hash_str("chile")),
    (hash_str("cn"), hash_str("china")),
    (hash_str("co"), hash_str("colombia")),
    (hash_str("cr"), hash_str("costa")),
    (hash_str("hr"), hash_str("croatia")),
    (hash_str("cu"), hash_str("cuba")),
    (hash_str("cy"), hash_str("cyprus")),
    (hash_str("cz"), hash_str("czechia")),
    (hash_str("dk"), hash_str("denmark")),
    (hash_str("ec"), hash_str("ecuador")),
    (hash_str("eg"), hash_str("egypt")),
    (hash_str("ee"), hash_str("estonia")),
    (hash_str("et"), hash_str("ethiopia")),
    (hash_str("fi"), hash_str("finland")),
    (hash_str("fr"), hash_str("france")),
    (hash_str("ge"), hash_str("georgia")),
    (hash_str("de"), hash_str("germany")),
    (hash_str("gh"), hash_str("ghana")),
    (hash_str("gr"), hash_str("greece")),
    (hash_str("gl"), hash_str("greenland")),
    (hash_str("gt"), hash_str("guatemala")),
    (hash_str("hn"), hash_str("honduras")),
    (hash_str("hu"), hash_str("hungary")),
    (hash_str("is"), hash_str("iceland")),
    (hash_str("in"), hash_str("india")),
    (hash_str("id"), hash_str("indonesia")),
    (hash_str("ir"), hash_str("iran")),
    (hash_str("iq"), hash_str("iraq")),
    (hash_str("ie"), hash_str("ireland")),
    (hash_str("il"), hash_str("israel")),
    (hash_str("it"), hash_str("italy")),
    (hash_str("jm"), hash_str("jamaica")),
    (hash_str("jp"), hash_str("japan")),
    (hash_str("jo"), hash_str("jordan")),
    (hash_str("kz"), hash_str("kazakhstan")),
    (hash_str("ke"), hash_str("kenya")),
    (hash_str("kw"), hash_str("kuwait")),
    (hash_str("lv"), hash_str("latvia")),
    (hash_str("lb"), hash_str("lebanon")),
    (hash_str("ly"), hash_str("libya")),
    (hash_str("lt"), hash_str("lithuania")),
    (hash_str("lu"), hash_str("luxembourg")),
    (hash_str("mg"), hash_str("madagascar")),
    (hash_str("my"), hash_str("malaysia")),
    (hash_str("mt"), hash_str("malta")),
    (hash_str("mx"), hash_str("mexico")),
    (hash_str("md"), hash_str("moldova")),
    (hash_str("mc"), hash_str("monaco")),
    (hash_str("mn"), hash_str("mongolia")),
    (hash_str("me"), hash_str("montenegro")),
    (hash_str("ma"), hash_str("morocco")),
    (hash_str("mz"), hash_str("mozambique")),
    (hash_str("mm"), hash_str("myanmar")),
    (hash_str("na"), hash_str("namibia")),
    (hash_str("np"), hash_str("nepal")),
    (hash_str("nz"), hash_str("zealand")),
    (hash_str("ni"), hash_str("nicaragua")),
    (hash_str("ng"), hash_str("nigeria")),
    (hash_str("no"), hash_str("norway")),
    (hash_str("om"), hash_str("oman")),
    (hash_str("pk"), hash_str("pakistan")),
    (hash_str("pa"), hash_str("panama")),
    (hash_str("py"), hash_str("paraguay")),
    (hash_str("pe"), hash_str("peru")),
    (hash_str("ph"), hash_str("philippines")),
    (hash_str("pl"), hash_str("poland")),
    (hash_str("pt"), hash_str("portugal")),
    (hash_str("qa"), hash_str("qatar")),
    (hash_str("ro"), hash_str("romania")),
    (hash_str("ru"), hash_str("russia")),
    (hash_str("rw"), hash_str("rwanda")),
    (hash_str("sa"), hash_str("arabia")),
    (hash_str("sn"), hash_str("senegal")),
    (hash_str("rs"), hash_str("serbia")),
    (hash_str("sg"), hash_str("singapore")),
    (hash_str("sk"), hash_str("slovakia")),
    (hash_str("si"), hash_str("slovenia")),
    (hash_str("so"), hash_str("somalia")),
    (hash_str("es"), hash_str("spain")),
    (hash_str("lk"), hash_str("lanka")),
    (hash_str("sd"), hash_str("sudan")),
    (hash_str("se"), hash_str("sweden")),
    (hash_str("ch"), hash_str("switzerland")),
    (hash_str("sy"), hash_str("syria")),
    (hash_str("tw"), hash_str("taiwan")),
    (hash_str("tz"), hash_str("tanzania")),
    (hash_str("th"), hash_str("thailand")),
    (hash_str("tn"), hash_str("tunisia")),
    (hash_str("tr"), hash_str("turkey")),
    (hash_str("ug"), hash_str("uganda")),
    (hash_str("ua"), hash_str("ukraine")),
    (hash_str("uy"), hash_str("uruguay")),
    (hash_str("uz"), hash_str("uzbekistan")),
    (hash_str("ve"), hash_str("venezuela")),
    (hash_str("vn"), hash_str("vietnam")),
    (hash_str("zm"), hash_str("zambia")),
    (hash_str("zw"), hash_str("zimbabwe")),
];

/// The alpha-2 code for a country name, if this token names one.
pub fn code_for_name(name: u32) -> Option<u32> {
    let mut i = 0usize;
    while i < CODES.len() {
        if CODES[i].1 == name {
            return Some(CODES[i].0);
        }
        i += 1;
    }
    None
}

/// The country name for an alpha-2 code, if this token is one.
pub fn name_for_code(code: u32) -> Option<u32> {
    let mut i = 0usize;
    while i < CODES.len() {
        if CODES[i].0 == code {
            return Some(CODES[i].1);
        }
        i += 1;
    }
    None
}

/// Is this token a country code we can check? A code we know is a **claim**: it
/// either matches the ground truth or contradicts it. A two-letter token we do
/// not know (`IP`, `AS`, `RIR`) is method vocabulary and abstains instead.
#[cfg(test)]
pub fn is_country_code(code: u32) -> bool {
    name_for_code(code).is_some()
}

/// `(alpha-2 subdivision code, single-token name)` for US states and Canadian
/// provinces, as case-folded FNV-1a hashes.
///
/// Why this is separate from `CODES`. The module's own limitation note said
/// subdivision codes were "not modelled at all", and for IP geolocation that is
/// the most common abbreviation there is. Worse than missing: **`CA` is already
/// a country code (Canada)**, so `Mountain View, CA, United States` read `CA` as
/// a claim that the country is Canada and scored it as a contradiction against a
/// truth of California. Measured before this table, that one substitution took a
/// verbatim-correct answer from 1.0000 to **0.3297** — a heavier penalty than a
/// genuinely wrong organisation (0.2980), which is an ordering inversion, not a
/// harsh-but-defensible score.
///
/// Only single-token names are here, for the same reason as `CODES`: the initials
/// rule already turns `New York` into `NY` and `North Carolina` into `NC`, so
/// multi-word states need no table.
///
/// A code that names both a country and a state (`CA`, `DE`, `IN`, `LA`, `MT`)
/// now indexes **both** readings. That is the correct semantics for an ambiguous
/// token: it is support for whichever reading the ground truth actually contains,
/// and only contradicts when the truth contains neither. A bare `DE` against a
/// truth of Uruguay still costs, because Uruguay names neither Germany nor
/// Delaware.
static STATES: [(u32, u32); 47] = [
    (hash_str("al"), hash_str("alabama")),
    (hash_str("ak"), hash_str("alaska")),
    (hash_str("az"), hash_str("arizona")),
    (hash_str("ar"), hash_str("arkansas")),
    (hash_str("ca"), hash_str("california")),
    (hash_str("co"), hash_str("colorado")),
    (hash_str("ct"), hash_str("connecticut")),
    (hash_str("de"), hash_str("delaware")),
    (hash_str("fl"), hash_str("florida")),
    (hash_str("ga"), hash_str("georgia")),
    (hash_str("hi"), hash_str("hawaii")),
    (hash_str("id"), hash_str("idaho")),
    (hash_str("il"), hash_str("illinois")),
    (hash_str("in"), hash_str("indiana")),
    (hash_str("ia"), hash_str("iowa")),
    (hash_str("ks"), hash_str("kansas")),
    (hash_str("ky"), hash_str("kentucky")),
    (hash_str("la"), hash_str("louisiana")),
    (hash_str("me"), hash_str("maine")),
    (hash_str("md"), hash_str("maryland")),
    (hash_str("ma"), hash_str("massachusetts")),
    (hash_str("mi"), hash_str("michigan")),
    (hash_str("mn"), hash_str("minnesota")),
    (hash_str("ms"), hash_str("mississippi")),
    (hash_str("mo"), hash_str("missouri")),
    (hash_str("mt"), hash_str("montana")),
    (hash_str("ne"), hash_str("nebraska")),
    (hash_str("nv"), hash_str("nevada")),
    (hash_str("oh"), hash_str("ohio")),
    (hash_str("ok"), hash_str("oklahoma")),
    (hash_str("or"), hash_str("oregon")),
    (hash_str("pa"), hash_str("pennsylvania")),
    (hash_str("tn"), hash_str("tennessee")),
    (hash_str("tx"), hash_str("texas")),
    (hash_str("ut"), hash_str("utah")),
    (hash_str("vt"), hash_str("vermont")),
    (hash_str("va"), hash_str("virginia")),
    (hash_str("wa"), hash_str("washington")),
    (hash_str("wi"), hash_str("wisconsin")),
    (hash_str("wy"), hash_str("wyoming")),
    // Canadian provinces, named in the same limitation note. SK is also
    // Slovakia and NU also Niue, which the both-readings rule already handles.
    (hash_str("ab"), hash_str("alberta")),
    (hash_str("mb"), hash_str("manitoba")),
    (hash_str("on"), hash_str("ontario")),
    (hash_str("qc"), hash_str("quebec")),
    (hash_str("sk"), hash_str("saskatchewan")),
    (hash_str("yt"), hash_str("yukon")),
    (hash_str("nu"), hash_str("nunavut")),
];

/// The subdivision code for a US state name, if this token names one.
pub fn state_code_for_name(name: u32) -> Option<u32> {
    let mut i = 0usize;
    while i < STATES.len() {
        if STATES[i].1 == name {
            return Some(STATES[i].0);
        }
        i += 1;
    }
    None
}

/// The US state name for a subdivision code, if this token is one.
pub fn state_name_for_code(code: u32) -> Option<u32> {
    let mut i = 0usize;
    while i < STATES.len() {
        if STATES[i].0 == code {
            return Some(STATES[i].1);
        }
        i += 1;
    }
    None
}

/// Is this token a checkable place code — a country or a US state? Used by the
/// abstention rule, so a wrong state code costs like a wrong country code.
pub fn is_place_code(code: u32) -> bool {
    name_for_code(code).is_some() || state_name_for_code(code).is_some()
}

/// Corporate-form suffixes, as case-folded FNV-1a hashes.
///
/// `LLC`, `Inc` and `Ltd` are not entities; they are the legal form attached to
/// one. Counting them as ground-truth entity mass means an answer that names
/// the company correctly but drops the suffix leaves mass uncovered, and that
/// uncovered mass is exactly what turns an answer's extra true detail into a
/// charged substitution.
///
/// Measured against a truth of "... operated by Google LLC (AS15169)": the
/// answer "8.8.8.8 is Google Public DNS, hosted in Mountain View, California,
/// United States" — correct, and natural phrasing — scored **0.2459**, below
/// the 0.3057 given to an answer naming the wrong city. Saying "operated by
/// Google" with no suffix already scored 1.0000, so the penalty was not for
/// omitting the company; it was for volunteering "Public DNS" while `LLC` sat
/// unmatched.
///
/// Only unambiguous suffixes are listed. `Co`, `SA`, `AG` and `AS` are left out
/// on purpose: they collide with Colorado, country codes and the autonomous
/// system marker, and a wrong reading there would be worse than the omission.
static CORP_SUFFIXES: [u32; 11] = [
    hash_str("llc"),
    hash_str("inc"),
    hash_str("incorporated"),
    hash_str("ltd"),
    hash_str("limited"),
    hash_str("corp"),
    hash_str("corporation"),
    hash_str("gmbh"),
    hash_str("plc"),
    hash_str("llp"),
    hash_str("pty"),
];

/// Is this token a corporate-form suffix rather than an entity?
pub fn is_corporate_suffix(token: u32) -> bool {
    let mut i = 0usize;
    while i < CORP_SUFFIXES.len() {
        if CORP_SUFFIXES[i] == token {
            return true;
        }
        i += 1;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_and_names_round_trip() {
        assert_eq!(code_for_name(hash_str("uruguay")), Some(hash_str("uy")));
        assert_eq!(name_for_code(hash_str("de")), Some(hash_str("germany")));
        assert_eq!(name_for_code(hash_str("JP")), Some(hash_str("japan")));
    }

    #[test]
    fn method_vocabulary_is_not_a_country_code() {
        for word in ["ip", "as", "id_", "xx", "qq"] {
            assert!(!is_country_code(hash_str(word)), "{word} read as a country");
        }
        // `id` really is Indonesia, and that is the correct reading: an answer
        // writing a bare `ID` where the truth says Indonesia should get credit.
        assert!(is_country_code(hash_str("id")));
    }

    #[test]
    fn us_state_codes_resolve_in_both_directions() {
        assert_eq!(
            state_code_for_name(hash_str("california")),
            Some(hash_str("ca"))
        );
        assert_eq!(state_name_for_code(hash_str("TX")), Some(hash_str("texas")));
        // Multi-word states are reached by the initials rule, not this table.
        assert_eq!(state_name_for_code(hash_str("ny")), None);
    }

    #[test]
    fn a_code_naming_both_a_country_and_a_state_keeps_both_readings() {
        // CA is Canada and California; DE is Germany and Delaware. Losing
        // either reading is what made "Mountain View, CA" score as a claim
        // that the country was Canada.
        for (code, country, state) in [
            ("ca", "canada", "california"),
            ("de", "germany", "delaware"),
        ] {
            assert_eq!(name_for_code(hash_str(code)), Some(hash_str(country)));
            assert_eq!(state_name_for_code(hash_str(code)), Some(hash_str(state)));
            assert!(is_place_code(hash_str(code)));
        }
    }

    #[test]
    fn method_vocabulary_is_still_not_a_place_code() {
        for word in ["ip", "as", "xx", "qq"] {
            assert!(!is_place_code(hash_str(word)), "{word} read as a place");
        }
    }

    #[test]
    fn canadian_provinces_resolve_too() {
        assert_eq!(
            state_code_for_name(hash_str("ontario")),
            Some(hash_str("on"))
        );
        assert_eq!(
            state_name_for_code(hash_str("QC")),
            Some(hash_str("quebec"))
        );
        // SK is Slovakia as well as Saskatchewan; both readings stay available.
        assert_eq!(name_for_code(hash_str("sk")), Some(hash_str("slovakia")));
        assert_eq!(
            state_name_for_code(hash_str("sk")),
            Some(hash_str("saskatchewan"))
        );
    }

    #[test]
    fn corporate_suffixes_are_not_entities() {
        for w in ["llc", "Inc", "LTD", "GmbH", "plc"] {
            assert!(is_corporate_suffix(hash_str(w)), "{w} not recognised");
        }
        // Ambiguous forms stay out: Co is Colorado, AS is the ASN marker.
        for w in ["co", "as", "sa", "ag", "google"] {
            assert!(
                !is_corporate_suffix(hash_str(w)),
                "{w} wrongly treated as a suffix"
            );
        }
    }

    #[test]
    fn no_code_is_listed_twice() {
        let mut i = 0usize;
        while i < CODES.len() {
            let mut j = i + 1;
            while j < CODES.len() {
                assert_ne!(CODES[i].0, CODES[j].0, "duplicate code at {i}/{j}");
                assert_ne!(CODES[i].1, CODES[j].1, "duplicate name at {i}/{j}");
                j += 1;
            }
            i += 1;
        }
    }
}

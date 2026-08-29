//! Tokenisation and salience weighting.
//!
//! Split on non-word bytes, but keep `.` `,` `-` `:` `/` `_` *inside* a token
//! when both neighbours are alphanumeric and a digit is involved. That single
//! rule is what keeps `192.168.1.10`, `CVE-2021-44228`, `142.250.0.0/15`,
//! `2026-08-27`, `1,000` and `3.14` intact — the tokens that decide Tier-A
//! correctness. Bytes >= 0x80 are opaque word bytes, never decoded, so emoji /
//! CJK / accented input cannot trap (A1 Stage-1).

#![allow(dead_code)]

use crate::bytes::unit_family_hash;
use crate::bytes::*;
use crate::profile::profile;
use crate::units::{
    leading_range, suffix_is_negative_hemisphere, unit_word_code, P_BASE, U_DEG, U_NONE,
};

/// Ample: a live `converted_answer` runs ~50 tokens and the longest ground truth
/// in the corpus is a few hundred. Only the ~54 KB adversarial Stage-1 input
/// reaches this, where the requirement is merely not to trap.
pub const MAX_TOKENS: usize = 2048;

pub const K_WORD: u8 = 0;
pub const K_NUMBER: u8 = 1;
pub const K_IDENT: u8 = 2;

pub struct Toks {
    pub n: usize,
    pub hash: [u32; MAX_TOKENS],
    pub stem: [u32; MAX_TOKENS],
    /// Four-letter family hash, used only for unrecognised unit-words.
    pub family: [u32; MAX_TOKENS],
    pub w: [f32; MAX_TOKENS],
    pub val: [f32; MAX_TOKENS],
    /// Upper bound of a hyphenated range; equal to `val` for a plain figure.
    pub vhi: [f32; MAX_TOKENS],
    pub kind: [u8; MAX_TOKENS],
    pub unit: [u8; MAX_TOKENS],
    /// Unit this token *names* on its own (`km`, `knots`, `°C`), precomputed so
    /// the unit pass never needs the original bytes back.
    pub uword: [u8; MAX_TOKENS],
    /// Source byte immediately after the token, so a trailing `%` is not lost.
    pub nb: [u8; MAX_TOKENS],
    /// Hash of a neighbouring word that sits where a unit would but names no
    /// unit we know (`hPa`, `bananas`). Zero when there is none.
    pub ufword: [u32; MAX_TOKENS],
    /// Token falls under a negation that has not been closed by a clause break.
    pub neg: [bool; MAX_TOKENS],
    /// First letter, lower-cased, so a run of proper nouns can be reduced to the
    /// acronym a miner may legitimately use instead ("United States" -> "us").
    pub first: [u8; MAX_TOKENS],
    /// Capitalised mid-sentence: a proper noun, i.e. a salient entity.
    pub proper: [bool; MAX_TOKENS],
    /// A **two-letter** ALL-CAPS token: a standard code, not a name. ISO 3166
    /// country codes and US/Canadian state codes are exactly two letters ("US",
    /// "UY", "IS", "CA", "NY"), and their expansion cannot be derived from the
    /// spelling — "UY" is not reachable from "Uruguay" by any lexical rule, so
    /// the acronym pass in `score.rs` (which builds initials from a *run* of
    /// proper nouns) can never produce it. Such a token has to abstain rather
    /// than read as a wrong entity.
    ///
    /// The length bound is what makes this safe. It used to cover every ALL-CAPS
    /// token, and a wrong ISP written as an acronym went free: "operated by AWS"
    /// against a truth of "Google LLC" scored 0.9829 while the same swap spelled
    /// "Cloudflare Inc." scored 0.2248. Organisation acronyms are three letters
    /// or more; standard geographic codes are two.
    pub abbrev: [bool; MAX_TOKENS],
    /// Carries an assertion rather than prose: a figure, an identifier, or a
    /// proper noun. These are what a Tier-A answer is right or wrong about.
    pub decisive: [bool; MAX_TOKENS],
    pub boiler: [bool; MAX_TOKENS],
    pub echo: [bool; MAX_TOKENS],
    /// How well the ground truth supports this token, in [0,1]. Graded, not
    /// boolean: a figure 1% off must not read the same as one that is absent.
    pub supw: [f32; MAX_TOKENS],
    /// Index of the ground-truth token this one matched, for polarity checks.
    pub supi: [u32; MAX_TOKENS],
    pub has_ident: bool,
}

pub const EMPTY_TOKS: Toks = Toks {
    n: 0,
    hash: [0; MAX_TOKENS],
    stem: [0; MAX_TOKENS],
    family: [0; MAX_TOKENS],
    w: [0.0; MAX_TOKENS],
    val: [0.0; MAX_TOKENS],
    vhi: [0.0; MAX_TOKENS],
    kind: [K_WORD; MAX_TOKENS],
    unit: [U_NONE; MAX_TOKENS],
    uword: [U_NONE; MAX_TOKENS],
    nb: [0; MAX_TOKENS],
    ufword: [0; MAX_TOKENS],
    neg: [false; MAX_TOKENS],
    first: [0; MAX_TOKENS],
    proper: [false; MAX_TOKENS],
    abbrev: [false; MAX_TOKENS],
    decisive: [false; MAX_TOKENS],
    boiler: [false; MAX_TOKENS],
    echo: [false; MAX_TOKENS],
    supw: [0.0; MAX_TOKENS],
    supi: [0; MAX_TOKENS],
    has_ident: false,
};

impl Toks {
    pub const fn new() -> Toks {
        EMPTY_TOKS
    }
}

// --------------------------------------------------------------------------
// Salience
// --------------------------------------------------------------------------

/// ~90 function words. A stopword still weighs a little, so that padding an
/// answer with them dilutes its precision denominator instead of being free.
const STOPWORDS: [u32; 92] = [
    hash_str("the"),
    hash_str("a"),
    hash_str("an"),
    hash_str("and"),
    hash_str("or"),
    hash_str("but"),
    hash_str("if"),
    hash_str("of"),
    hash_str("to"),
    hash_str("in"),
    hash_str("on"),
    hash_str("at"),
    hash_str("by"),
    hash_str("for"),
    hash_str("with"),
    hash_str("from"),
    hash_str("as"),
    hash_str("is"),
    hash_str("are"),
    hash_str("was"),
    hash_str("were"),
    hash_str("be"),
    hash_str("been"),
    hash_str("being"),
    hash_str("am"),
    hash_str("has"),
    hash_str("have"),
    hash_str("had"),
    hash_str("do"),
    hash_str("does"),
    hash_str("did"),
    hash_str("will"),
    hash_str("would"),
    hash_str("shall"),
    hash_str("should"),
    hash_str("can"),
    hash_str("could"),
    hash_str("may"),
    hash_str("might"),
    hash_str("must"),
    hash_str("this"),
    hash_str("that"),
    hash_str("these"),
    hash_str("those"),
    hash_str("it"),
    hash_str("its"),
    hash_str("they"),
    hash_str("them"),
    hash_str("their"),
    hash_str("there"),
    hash_str("here"),
    hash_str("what"),
    hash_str("which"),
    hash_str("who"),
    hash_str("whom"),
    hash_str("whose"),
    hash_str("when"),
    hash_str("where"),
    hash_str("why"),
    hash_str("how"),
    hash_str("all"),
    hash_str("any"),
    hash_str("both"),
    hash_str("each"),
    hash_str("few"),
    hash_str("more"),
    hash_str("most"),
    hash_str("some"),
    hash_str("such"),
    hash_str("than"),
    hash_str("too"),
    hash_str("very"),
    hash_str("just"),
    hash_str("also"),
    hash_str("into"),
    hash_str("over"),
    hash_str("under"),
    hash_str("about"),
    hash_str("between"),
    hash_str("during"),
    hash_str("you"),
    hash_str("your"),
    hash_str("i"),
    hash_str("we"),
    hash_str("our"),
    hash_str("he"),
    hash_str("she"),
    hash_str("his"),
    hash_str("her"),
    hash_str("not"),
    hash_str("no"),
    hash_str("s"),
];

fn is_stopword(h: u32) -> bool {
    let mut i = 0usize;
    while i < STOPWORDS.len() {
        if STOPWORDS[i] == h {
            return true;
        }
        i += 1;
    }
    false
}

/// Negators. `not` and `no` are also stopwords, so before this table they
/// weighed 0.05 out of a ~15-token pool and a sentence tied its own negation at
/// 1.0000 (adversarial review C2). Polarity is not a weighting question.
const NEGATORS: [u32; 14] = [
    hash_str("not"),
    hash_str("no"),
    hash_str("never"),
    hash_str("none"),
    hash_str("cannot"),
    hash_str("cant"),
    hash_str("wont"),
    hash_str("didnt"),
    hash_str("doesnt"),
    hash_str("isnt"),
    hash_str("arent"),
    hash_str("without"),
    hash_str("nor"),
    hash_str("neither"),
];

fn is_negator(h: u32) -> bool {
    let mut i = 0usize;
    while i < NEGATORS.len() {
        if NEGATORS[i] == h {
            return true;
        }
        i += 1;
    }
    false
}

/// How many following tokens a negator reaches over, before a clause boundary
/// closes it. "no longer" and "n't" both land here as plain negator tokens.
const NEG_WINDOW: i32 = 5;

fn weight(tok: &[u8], hash: u32, kind: u8, proper: bool, high: bool) -> f32 {
    let p = profile();
    if kind == K_NUMBER {
        return p.w_number;
    }
    if kind == K_IDENT {
        return p.w_ident;
    }
    if is_stopword(hash) {
        return p.w_stop;
    }
    if high {
        // A script we cannot segment: real content, but we cannot say how much.
        return p.w_high;
    }
    let len = if tok.len() as f32 > p.w_len_cap {
        p.w_len_cap
    } else {
        tok.len() as f32
    };
    let mut w = p.w_word_base + p.w_len_step * len;
    if proper {
        w += p.w_proper;
    }
    w
}

// --------------------------------------------------------------------------
// Tokenise
// --------------------------------------------------------------------------

pub fn tokenize(src: &[u8], t: &mut Toks) {
    t.n = 0;
    t.has_ident = false;
    let n = src.len();
    let mut i = 0usize;
    let mut negwin: i32 = 0;
    // A capital that opens a sentence says nothing about proper-noun-hood.
    let mut sentence_start = true;

    while i < n && t.n < MAX_TOKENS {
        if !is_wordbyte(src[i]) {
            // A clause boundary ends a negation's reach: in "No, the cert
            // expired" the negation applies to the verdict, not to "expired".
            let b = src[i];
            if b == b'.' || b == b',' || b == b';' || b == b'!' || b == b'?' || b == b':' {
                negwin = 0;
            }
            if b == b'.' || b == b'!' || b == b'?' {
                sentence_start = true;
            }
            i += 1;
            continue;
        }
        let start = i;
        let (mut has_alpha, mut has_digit, mut high) = (false, false, false);
        let mut seps = 0u8;
        let mut digits = 0u8;

        while i < n {
            let b = src[i];
            if is_wordbyte(b) {
                if is_alpha(b) {
                    has_alpha = true;
                } else if is_digit(b) {
                    has_digit = true;
                    digits = digits.saturating_add(1);
                } else {
                    high = true;
                }
                i += 1;
            } else if is_sep(b)
                && i + 1 < n
                && is_alnum(src[i - 1])
                && is_alnum(src[i + 1])
                && (has_digit || is_digit(src[i + 1]))
            {
                seps += 1;
                i += 1;
            } else {
                break;
            }
        }

        let tok = &src[start..i];
        if tok.is_empty() {
            continue;
        }

        // Classify. A leading decimal run followed by nothing or by a known unit
        // is a figure; anything else mixing letters and digits, or carrying two
        // or more internal separators, is an identifier (IP, CVE id, version,
        // date) and admits no numeric tolerance.
        let (mut val, mut vhi, used) = leading_range(tok);
        let rest = &tok[used..];
        let suffix_unit = if rest.is_empty() {
            U_NONE
        } else {
            unit_word_code(rest)
        };
        // A bare decimal carrying only a hemisphere letter is a coordinate, not
        // an identifier: `34.9011S` must parse as -34.9011 rather than falling
        // through to K_IDENT where no tolerance applies (adversarial review M5).
        let hemi = used > 0 && rest.len() == 1 && is_hemisphere(rest[0]);
        let kind = if used > 0 && (rest.is_empty() || suffix_unit != U_NONE || hemi) {
            K_NUMBER
        } else if (has_alpha && has_digit && (digits >= 2 || seps >= 1)) || (has_digit && seps >= 2)
        {
            // A lone digit embedded in a word is part of a name, not an
            // identifier: `IPv4`, `IP2Location` and `S3` are vocabulary, while
            // `AS15169`, `8.8.8.8` and `CVE-2024-1234` carry two or more digits
            // or a separator. Treating the former as identifiers put them in a
            // channel that admits no tolerance, so an answer citing IP2Location
            // as its source was scored as asserting a wrong identifier.
            K_IDENT
        } else if has_digit {
            K_NUMBER
        } else {
            K_WORD
        };
        if hemi && (lower(rest[0]) == b's' || lower(rest[0]) == b'w') {
            val = -val;
            vhi = val;
        }

        // `104.8669°W` is a western longitude: negative, not positive.
        if kind == K_NUMBER && suffix_is_negative_hemisphere(rest) {
            val = -val;
            vhi = val;
        }

        // A leading `-` that is a sign rather than a hyphen. Longitudes and
        // negative wind components depend on this: -122.4194 is not 122.4194.
        if kind == K_NUMBER
            && start > 0
            && src[start - 1] == b'-'
            && (start < 2 || !is_alnum(src[start - 2]))
        {
            let span = vhi - val;
            val = -val;
            vhi = val + span;
        }

        let h = hash_bytes(tok);
        // Sentence-initial capitals are not entities. Without this, a verbose
        // answer written as several sentences ("Sustained wind ... Peak gusts
        // ... Precipitation totals ...") reads every sentence opener as a proper
        // noun the ground truth never states, and the entity channel scores a
        // wholly correct answer as a pile of contradictions.
        let proper = start > 0 && has_alpha && tok[0].is_ascii_uppercase() && !sentence_start;
        let k = t.n;

        t.hash[k] = h;
        t.stem[k] = if kind == K_WORD { stem_hash(tok) } else { h };
        t.family[k] = if kind == K_WORD {
            unit_family_hash(tok)
        } else {
            h
        };
        t.kind[k] = kind;
        t.val[k] = val;
        t.vhi[k] = if vhi >= val { vhi } else { val };
        // A partial unit (`km` awaiting an `h`) is not a unit on its own.
        t.unit[k] = if kind == K_NUMBER && suffix_unit < P_BASE {
            if hemi {
                U_DEG
            } else {
                suffix_unit
            }
        } else {
            U_NONE
        };
        t.uword[k] = unit_word_code(tok);
        t.nb[k] = if i < n { src[i] } else { 0 };
        t.w[k] = weight(tok, h, kind, proper, high);
        // Every per-token field is written on every push: a field left over from
        // a previous call would make the score depend on call order.
        t.decisive[k] = kind != K_WORD || proper;
        t.proper[k] = proper && kind == K_WORD;
        t.abbrev[k] = kind == K_WORD && all_upper(tok) && tok.len() <= 2;
        t.first[k] = if is_alpha(tok[0]) { lower(tok[0]) } else { 0 };
        t.boiler[k] = false;
        t.echo[k] = false;
        t.supw[k] = 0.0;
        t.supi[k] = 0;
        t.ufword[k] = 0;
        t.neg[k] = negwin > 0;
        if kind == K_IDENT {
            t.has_ident = true;
        }
        // A negator opens a window over what follows; anything else inside an
        // open window counts down toward the clause it belongs to.
        if kind == K_WORD && is_negator(h) {
            negwin = NEG_WINDOW;
            t.neg[k] = true;
        } else if negwin > 0 {
            negwin -= 1;
        }
        sentence_start = false;
        t.n = k + 1;
    }
}

/// Every alphabetic byte upper-case, and there is at least one.
fn all_upper(tok: &[u8]) -> bool {
    let mut seen = false;
    let mut i = 0usize;
    while i < tok.len() {
        if is_alpha(tok[i]) {
            if !tok[i].is_ascii_uppercase() {
                return false;
            }
            seen = true;
        }
        i += 1;
    }
    seen
}

// --------------------------------------------------------------------------
// Boilerplate openers
// --------------------------------------------------------------------------

/// The measured opening-phrase histogram of `converted_answer`: 86.9% of live
/// answers open literally "The data ..." (gate analysis §4.1). These carry no
/// information about the answer, so they are struck from both sides of the
/// precision ratio rather than being allowed to inflate it.
/// Only genuinely contentless openers appear here. Four weather-specific
/// entries (`the weather forecast`, `the current weather`, `the forecast for`,
/// `the weather in`) were removed: `weather` and `forecast` are *content* words
/// for a weather intent, so striking them at position 0 scored one phrasing
/// differently from another — worth up to +0.02 on the storm build — which is a
/// phrasing match, and the Rule-04 disclosure says no phrasing is matched
/// (adversarial review M9).
const BOILER: [&[u32]; 8] = [
    &[hash_str("the"), hash_str("data"), hash_str("shows")],
    &[hash_str("the"), hash_str("data"), hash_str("provides")],
    &[hash_str("the"), hash_str("data"), hash_str("indicates")],
    &[hash_str("the"), hash_str("data"), hash_str("describes")],
    &[hash_str("this"), hash_str("data"), hash_str("shows")],
    &[hash_str("this"), hash_str("data"), hash_str("describes")],
    &[hash_str("the"), hash_str("data")],
    &[hash_str("this"), hash_str("data")],
];

/// Strike the longest matching opener from the head of the answer.
pub fn mark_boilerplate(t: &mut Toks) {
    let mut best = 0usize;
    let mut p = 0usize;
    while p < BOILER.len() {
        let phrase = BOILER[p];
        if phrase.len() > best && phrase.len() <= t.n {
            let mut j = 0usize;
            let mut ok = true;
            while j < phrase.len() {
                if t.hash[j] != phrase[j] {
                    ok = false;
                    break;
                }
                j += 1;
            }
            if ok {
                best = phrase.len();
            }
        }
        p += 1;
    }
    let mut j = 0usize;
    while j < best {
        t.boiler[j] = true;
        j += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toks(s: &[u8]) -> Toks {
        let mut t = Toks::new();
        tokenize(s, &mut t);
        t
    }

    #[test]
    fn identifiers_survive_tokenisation() {
        let t = toks(b"IP 192.168.1.10 and CVE-2021-44228 on 2026-08-27");
        let mut idents = 0;
        for k in 0..t.n {
            if t.kind[k] == K_IDENT {
                idents += 1;
            }
        }
        assert_eq!(idents, 3, "IP, CVE id and date must each stay one token");
        assert!(t.has_ident);
    }

    #[test]
    fn decimals_and_units_are_figures() {
        let t = toks(b"temperature 23.1C with risk 0.429 and 55%");
        let mut nums = 0;
        for k in 0..t.n {
            if t.kind[k] == K_NUMBER {
                nums += 1;
            }
        }
        assert_eq!(nums, 3);
    }

    #[test]
    fn negative_longitudes_keep_their_sign() {
        let t = toks(b"latitude 37.7749 and longitude -122.4194");
        let mut saw_neg = false;
        for k in 0..t.n {
            if t.kind[k] == K_NUMBER && t.val[k] < 0.0 {
                saw_neg = true;
            }
        }
        assert!(saw_neg, "a sign must not be dropped: -122 is not 122");
    }

    #[test]
    fn hyphenated_words_are_not_identifiers() {
        let t = toks(b"a well-known service");
        for k in 0..t.n {
            assert_ne!(t.kind[k], K_IDENT);
        }
    }

    #[test]
    fn high_bytes_never_trap_and_stay_opaque() {
        let t = toks("emoji \u{1F5FC} CJK \u{4E2D}\u{6587} accents caf\u{E9}".as_bytes());
        assert!(t.n > 0);
    }

    #[test]
    fn numbers_outweigh_stopwords_by_a_wide_margin() {
        let t = toks(b"the 10");
        assert!(t.w[1] > t.w[0] * 20.0);
    }

    #[test]
    fn boilerplate_openers_are_struck() {
        let mut t = toks(b"The data shows the IP is in Brisbane");
        mark_boilerplate(&mut t);
        assert!(t.boiler[0] && t.boiler[1] && t.boiler[2]);
        assert!(!t.boiler[3]);
    }

    #[test]
    fn truncation_is_bounded_not_a_trap() {
        // ~54 KB of repeated text, the Stage-1 adversarial case.
        let mut big = [0u8; 54 * 1024];
        let word = b"storm ";
        let mut i = 0usize;
        while i < big.len() {
            big[i] = word[i % word.len()];
            i += 1;
        }
        let t = toks(&big);
        assert!(t.n <= MAX_TOKENS);
    }
}

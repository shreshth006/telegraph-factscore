//! Byte classification, case folding, hashing, and the small float helpers.
//!
//! `core` has no `powf`/`exp`/`sqrt` (those live in `std`/libm), and importing a
//! math crate would add host imports. Every routine here is plain arithmetic, so
//! the whole scorer is transcendental-free — which is also what keeps it far
//! inside the node's 10-minute gate budget (ARCHITECTURE A2).

#![allow(dead_code)]

// --------------------------------------------------------------------------
// Byte classes. Bytes >= 0x80 are treated as opaque *word* bytes: we never
// decode UTF-8, so emoji/CJK/accented input can never trap (A1 Stage-1 trap).
// --------------------------------------------------------------------------

pub const fn lower(b: u8) -> u8 {
    if b >= b'A' && b <= b'Z' {
        b + 32
    } else {
        b
    }
}

pub const fn is_digit(b: u8) -> bool {
    b >= b'0' && b <= b'9'
}

pub const fn is_alpha(b: u8) -> bool {
    (b >= b'a' && b <= b'z') || (b >= b'A' && b <= b'Z')
}

pub const fn is_alnum(b: u8) -> bool {
    is_digit(b) || is_alpha(b)
}

/// Word byte: ASCII alphanumeric, or any high byte (opaque multi-byte script).
pub const fn is_wordbyte(b: u8) -> bool {
    is_alnum(b) || b >= 0x80
}

/// ASCII whitespace only. The Stage-1 whitespace test is exact equality to 0,
/// so this must match the host's notion of blank (space/tab/CR/LF/VT/FF).
pub const fn is_space(b: u8) -> bool {
    b == b' ' || b == b'\t' || b == b'\n' || b == b'\r' || b == 0x0b || b == 0x0c
}

/// Separators that may sit *inside* a token when both neighbours are alphanumeric
/// and a digit is involved: `192.168.1.10`, `CVE-2021-44228`, `142.250.0.0/15`,
/// `2026-08-27`, `1,000`, `3.14`.
pub const fn is_sep(b: u8) -> bool {
    b == b'.' || b == b',' || b == b'-' || b == b':' || b == b'/' || b == b'_'
}

// --------------------------------------------------------------------------
// Unicode punctuation folding
// --------------------------------------------------------------------------

/// If `src[i..]` begins with a Unicode punctuation or space character that has
/// an ASCII equivalent, return `(byte_length, ascii_equivalent)`.
///
/// Bytes >= 0x80 are otherwise opaque *word* bytes (never decoded, so emoji and
/// CJK cannot trap), which means a curly apostrophe glues its neighbours into
/// one token while the ASCII apostrophe splits them. Measured against a ground
/// truth reading `Shimo\u{2019}ochiai`, the identical answer written with an
/// ASCII `'` scored **0.2592** where the curly form scored 0.9997 — a correct
/// answer failed on typography alone. LLM output uses curly quotes and en-dashes
/// constantly, so this is not an edge case.
///
/// Only characters with a genuine ASCII equivalent are folded. Everything else
/// above 0x80 stays opaque.
pub const fn unicode_punct(src: &[u8], i: usize) -> Option<(usize, u8)> {
    let n = src.len();
    if i + 1 < n && src[i] == 0xC2 {
        // U+00A0 NO-BREAK SPACE, U+00AD SOFT HYPHEN
        match src[i + 1] {
            0xA0 => return Some((2, b' ')),
            0xAD => return Some((2, b'-')),
            _ => {}
        }
    }
    if i + 2 < n && src[i] == 0xE2 && src[i + 1] == 0x80 {
        match src[i + 2] {
            // U+2018/2019 single quotes, U+201A/201B variants, U+2032 prime
            0x98..=0x9B => return Some((3, b'\'')),
            // U+201C/201D/201E/201F double quotes
            0x9C..=0x9F => return Some((3, b'"')),
            // U+2010..U+2015 hyphens and dashes
            0x90..=0x95 => return Some((3, b'-')),
            // U+2026 ellipsis
            0xA6 => return Some((3, b'.')),
            // U+2000..U+200A spaces, U+2007 figure space, U+202F narrow NBSP
            0x80..=0x8A | 0xAF => return Some((3, b' ')),
            _ => {}
        }
    }
    if i + 2 < n && src[i] == 0xE2 && src[i + 1] == 0x80 && src[i + 2] == 0xB2 {
        return Some((3, b'\''));
    }
    // U+2212 MINUS SIGN
    if i + 2 < n && src[i] == 0xE2 && src[i + 1] == 0x88 && src[i + 2] == 0x92 {
        return Some((3, b'-'));
    }
    // U+3000 IDEOGRAPHIC SPACE
    if i + 2 < n && src[i] == 0xE3 && src[i + 1] == 0x80 && src[i + 2] == 0x80 {
        return Some((3, b' '));
    }
    None
}

/// Fold Unicode punctuation to ASCII into `dst`, returning the length written.
///
/// The mapping only ever shrinks the text (three bytes to one), so `dst` of the
/// source's length always suffices. Returns `None` when `dst` is too small,
/// where the caller falls back to the raw bytes: an over-long input is an
/// adversarial Stage-1 probe whose only requirement is not to trap.
pub fn fold_punct(src: &[u8], dst: &mut [u8]) -> Option<usize> {
    if src.len() > dst.len() {
        return None;
    }
    let mut i = 0usize;
    let mut o = 0usize;
    while i < src.len() {
        match unicode_punct(src, i) {
            Some((len, ascii)) => {
                dst[o] = ascii;
                o += 1;
                i += len;
            }
            None => {
                dst[o] = src[i];
                o += 1;
                i += 1;
            }
        }
    }
    Some(o)
}

// --------------------------------------------------------------------------
// FNV-1a over case-folded bytes. `const fn` so the stopword / boilerplate /
// unit tables are compile-time constants rather than runtime initialisation.
// --------------------------------------------------------------------------

const FNV_OFFSET: u32 = 0x811c_9dc5;
const FNV_PRIME: u32 = 0x0100_0193;

pub const fn hash_bytes(s: &[u8]) -> u32 {
    let mut h = FNV_OFFSET;
    let mut i = 0usize;
    while i < s.len() {
        h ^= lower(s[i]) as u32;
        h = h.wrapping_mul(FNV_PRIME);
        i += 1;
    }
    // 0 is reserved as "absent" in a few places; nudge it off.
    if h == 0 {
        1
    } else {
        h
    }
}

pub const fn hash_str(s: &str) -> u32 {
    hash_bytes(s.as_bytes())
}

/// Hash of the token with one common English suffix removed, so `provides` and
/// `provide`, or `ranges` and `range`, land on the same key. Deliberately crude:
/// a real stemmer is not worth the bytes, and over-stemming only ever costs a
/// little precision on a low-weight word.
/// Coarse "same word family" hash for **unit-shaped words only**.
///
/// The general stemmer cannot collapse `matches` and `matching`: the plural rule
/// yields `matche` while the -ing rule yields `match`, and widening the plural
/// rule to strip `es` would send `provides` to `provid` (see `stem_hash`). That
/// mismatch made a correct terse answer foreign to its own ground truth --
/// "7 matches" against "7 matching passages" fired `m_foreign_unit` and cost the
/// answer its numeric channel (fact 0.394 against 1.000 verbatim).
///
/// A four-letter prefix is deliberately blunt. It is applied ONLY to words that
/// sat where a unit would and named no unit we recognise, so the failure mode is
/// bounded: two *unrecognised* unit-words sharing four letters are treated as
/// the same quantity. Recognised units never reach here -- they carry a real
/// `U_*` code and are compared by dimension.
pub fn unit_family_hash(tok: &[u8]) -> u32 {
    let n = if tok.len() < 4 { tok.len() } else { 4 };
    hash_bytes(&tok[..n])
}

pub fn stem_hash(tok: &[u8]) -> u32 {
    let n = tok.len();
    if n < 5 {
        return hash_bytes(tok);
    }
    let cut = |k: usize| -> u32 { hash_bytes(&tok[..n - k]) };
    let e = |k: usize, s: &[u8]| -> bool {
        if n <= k {
            return false;
        }
        let mut i = 0usize;
        while i < k {
            if lower(tok[n - k + i]) != s[i] {
                return false;
            }
            i += 1;
        }
        true
    };
    if n > 6 && e(3, b"ing") {
        return cut(3);
    }
    if n > 5 && e(2, b"ed") {
        return cut(2);
    }
    if n > 5 && e(2, b"ly") {
        return cut(2);
    }
    // Plain plural only. Stripping "es" as a unit would send `provides` to
    // `provid` while `provide` stays put, so the two would never meet.
    if e(1, b"s") && !e(2, b"ss") {
        return cut(1);
    }
    hash_bytes(tok)
}

/// Equality up to case and whitespace only. Drives the exact-match shortcut that
/// pins `rank_answer(q, gt, gt)` to exactly 1.0 (A8 self-match ratchet).
///
/// It deliberately does **not** fold punctuation. An earlier version skipped
/// every non-word byte, which made `CVSS 10` and `CVSS 1.0`, `5.9 m/s` and
/// `59 m/s`, and `-122.4194` and `122.4194` "exact matches" that short-circuited
/// to a literal 1.0 before any scoring ran. Punctuation adjacent to digits is
/// load-bearing: it carries the decimal point, the sign, the dotted quad and the
/// thousands separator. The ratchet only needs identity, not tolerance.
pub fn normalized_equal(a: &[u8], b: &[u8]) -> bool {
    let (mut i, mut j) = (0usize, 0usize);
    loop {
        while i < a.len() && is_space(a[i]) {
            i += 1;
        }
        while j < b.len() && is_space(b[j]) {
            j += 1;
        }
        if i >= a.len() || j >= b.len() {
            return i >= a.len() && j >= b.len();
        }
        if lower(a[i]) != lower(b[j]) {
            return false;
        }
        i += 1;
        j += 1;
    }
}

/// Punctuation that ends a noun phrase. "Mountain View, California" is two
/// phrases, not one five-word name.
pub const fn is_phrase_break(b: u8) -> bool {
    b == b','
        || b == b'.'
        || b == b';'
        || b == b':'
        || b == b'!'
        || b == b'?'
        || b == b'('
        || b == b')'
        || b == b'\n'
        || b == b'|'
        || b == b'/'
}

/// A lone compass letter, which after a decimal marks a coordinate hemisphere.
pub const fn is_hemisphere(b: u8) -> bool {
    let l = lower(b);
    l == b'n' || l == b's' || l == b'e' || l == b'w'
}

/// True when every byte is ASCII whitespace (or the slice is empty).
pub fn is_blank(s: &[u8]) -> bool {
    let mut i = 0usize;
    while i < s.len() {
        if !is_space(s[i]) {
            return false;
        }
        i += 1;
    }
    true
}

// --------------------------------------------------------------------------
// Float helpers (core has no f32::abs / max / min without std).
// --------------------------------------------------------------------------

pub fn fabs(x: f32) -> f32 {
    if x < 0.0 {
        -x
    } else {
        x
    }
}

pub fn fmax(a: f32, b: f32) -> f32 {
    if a > b {
        a
    } else {
        b
    }
}

pub fn fmin(a: f32, b: f32) -> f32 {
    if a < b {
        a
    } else {
        b
    }
}

pub fn clamp01(x: f32) -> f32 {
    // Also collapses NaN to 0: every comparison with NaN is false, so the final
    // `else` arm is reached. The host does this too, but doing it here keeps the
    // breakdown export honest.
    if x > 1.0 {
        1.0
    } else if x > 0.0 {
        x
    } else {
        0.0
    }
}

/// Hermite smoothstep on [0,1]. Continuity, not cliffs (ARCHITECTURE A3.7).
pub fn smoothstep01(x: f32) -> f32 {
    let t = clamp01(x);
    t * t * (3.0 - 2.0 * t)
}

/// Smoothstep with knots: rescale [lo,hi] onto [0,1], then smooth. `hi <= lo`
/// degenerates to a threshold, so callers must keep the knots ordered.
pub fn smoothstep(lo: f32, hi: f32, x: f32) -> f32 {
    if hi <= lo {
        return if x >= hi { 1.0 } else { 0.0 };
    }
    smoothstep01((x - lo) / (hi - lo))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashing_is_case_insensitive() {
        assert_eq!(hash_str("Google"), hash_str("google"));
        assert_ne!(hash_str("google"), hash_str("gaggle"));
    }

    #[test]
    fn stemming_folds_common_suffixes() {
        assert_eq!(stem_hash(b"provides"), stem_hash(b"provide"));
        assert_eq!(stem_hash(b"ranges"), stem_hash(b"range"));
    }

    #[test]
    fn normalized_equal_folds_case_and_whitespace_only() {
        assert!(normalized_equal(
            b"The IP is 1.2.3.4.",
            b"the   ip is 1.2.3.4."
        ));
        assert!(normalized_equal(b"  Paris ", b"paris"));
        assert!(!normalized_equal(b"valid", b"invalid"));
        assert!(normalized_equal(b"", b"   "));
    }

    #[test]
    fn normalized_equal_never_folds_digit_punctuation() {
        // Each pair differs only in punctuation next to digits, and each pair is
        // a different claim. Folding these fired the exact-match shortcut and
        // returned a literal 1.0 for a wrong answer (adversarial review C1).
        assert!(!normalized_equal(b"The IP is 1.2.3.4.", b"the ip is 1234"));
        assert!(!normalized_equal(
            b"The CVSS score is 10.",
            b"The CVSS score is 1.0"
        ));
        assert!(!normalized_equal(b"winds of 5.9 m/s", b"winds of 59 m/s"));
        assert!(!normalized_equal(b"23.1 C", b"231 C"));
        assert!(!normalized_equal(b"-122.4194", b"122.4194"));
        assert!(!normalized_equal(b"1,000 reports", b"10.00 reports"));
        assert!(!normalized_equal(b"CVE-2021-44228", b"CVE-20-2144228"));
        assert!(!normalized_equal(b"192.168.1.10", b"192.168.11.0"));
    }

    #[test]
    fn blank_detection_matches_stage1() {
        assert!(is_blank(b""));
        assert!(is_blank(b" \t\r\n "));
        assert!(!is_blank(b"  x  "));
    }

    #[test]
    fn smoothstep_pins_the_endpoints() {
        // Self-match must survive calibration at exactly 1.0.
        assert_eq!(smoothstep01(1.0), 1.0);
        assert_eq!(smoothstep01(0.0), 0.0);
        assert_eq!(smoothstep(0.05, 0.80, 1.0), 1.0);
        assert_eq!(smoothstep(0.05, 0.80, 0.0), 0.0);
        // Monotone in between, and never a step.
        let a = smoothstep(0.05, 0.80, 0.3);
        let b = smoothstep(0.05, 0.80, 0.5);
        assert!(a > 0.0 && b > a && b < 1.0);
    }

    #[test]
    fn clamp01_collapses_nan() {
        assert_eq!(clamp01(f32::NAN), 0.0);
        assert_eq!(clamp01(f32::INFINITY), 1.0);
        assert_eq!(clamp01(-1.0), 0.0);
    }
}

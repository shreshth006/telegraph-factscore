//! Units: parsing figures, naming units, and normalising them.
//!
//! Two figures are only comparable inside one dimension. That single rule is
//! what stops a wrong "over the next 48 hours" from matching a ground-truth
//! gust of 47.3 km/h merely because the digits are close.

#![allow(dead_code)]

use crate::fs::bytes::*;
use crate::fs::tokens::{Toks, K_NUMBER, K_WORD};

// --------------------------------------------------------------------------
// Unit classes
// --------------------------------------------------------------------------

pub const U_NONE: u8 = 0;
pub const U_TEMP_C: u8 = 1;
pub const U_TEMP_F: u8 = 2;
pub const U_MS: u8 = 3;
pub const U_KMH: u8 = 4;
pub const U_KT: u8 = 5;
pub const U_MPH: u8 = 6;
pub const U_PCT: u8 = 7;
pub const U_DEG: u8 = 8;
pub const U_MM: u8 = 9;
pub const U_CM: u8 = 10;
pub const U_M: u8 = 11;
pub const U_KM: u8 = 12;
pub const U_IN: u8 = 13;
pub const U_FT: u8 = 14;
pub const U_SEC: u8 = 15;
pub const U_MIN: u8 = 16;
pub const U_HOUR: u8 = 17;
pub const U_DAY: u8 = 18;
pub const U_TEMP_K: u8 = 19;
pub const U_HPA: u8 = 20;
pub const U_MB: u8 = 21;
pub const U_INHG: u8 = 22;
pub const U_PSI: u8 = 23;
pub const U_BAR: u8 = 24;
pub const U_MI: u8 = 25;
pub const U_KG: u8 = 26;
pub const U_LB: u8 = 27;
pub const U_MSEC: u8 = 28;

/// Dimension of a unit. Two figures are only comparable within one dimension —
/// a temperature and a wind speed are not a near-miss, they are unrelated.
/// This is also what keeps a wrong "next 48 hours" from quietly matching a
/// ground-truth gust of 47.3 km/h just because the digits are close.
pub const D_NONE: u8 = 0;
pub const D_TEMP: u8 = 1;
pub const D_SPEED: u8 = 2;
pub const D_FRAC: u8 = 3;
pub const D_ANGLE: u8 = 4;
pub const D_LEN: u8 = 5;
pub const D_TIME: u8 = 6;
pub const D_PRESSURE: u8 = 7;
pub const D_MASS: u8 = 8;

pub fn dimension(u: u8) -> u8 {
    match u {
        U_TEMP_C | U_TEMP_F | U_TEMP_K => D_TEMP,
        U_MS | U_KMH | U_KT | U_MPH => D_SPEED,
        U_PCT => D_FRAC,
        U_DEG => D_ANGLE,
        U_MM | U_CM | U_M | U_KM | U_IN | U_FT | U_MI => D_LEN,
        U_SEC | U_MIN | U_HOUR | U_DAY | U_MSEC => D_TIME,
        U_HPA | U_MB | U_INHG | U_PSI | U_BAR => D_PRESSURE,
        U_KG | U_LB => D_MASS,
        _ => D_NONE,
    }
}

/// Convert to the canonical unit of the dimension: °C, m/s, fraction, degrees,
/// metres, seconds.
pub fn canonical(v: f32, u: u8) -> f32 {
    match u {
        U_TEMP_F => (v - 32.0) / 1.8,
        U_KMH => v / 3.6,
        U_KT => v * 0.514_444,
        U_MPH => v * 0.447_04,
        U_PCT => v / 100.0,
        U_MM => v / 1000.0,
        U_CM => v / 100.0,
        U_KM => v * 1000.0,
        U_IN => v * 0.0254,
        U_FT => v * 0.3048,
        U_MIN => v * 60.0,
        U_HOUR => v * 3600.0,
        U_DAY => v * 86400.0,
        U_MSEC => v / 1000.0,
        U_TEMP_K => v - 273.15,
        U_MI => v * 1609.344,
        // Pressure canonicalises to hPa, mass to kilograms.
        U_INHG => v * 33.8639,
        U_PSI => v * 68.9476,
        U_BAR => v * 1000.0,
        U_LB => v * 0.453_592,
        _ => v,
    }
}

/// Partial units: a unit word that only names a unit together with the next
/// token, because `km/h` and `m/s` tokenise apart when no digit touches the `/`.
/// Kept above every real unit code so one comparison separates the two kinds.
pub const P_BASE: u8 = 64;
pub const P_KM: u8 = 64;
pub const P_M: u8 = 65;
pub const P_S: u8 = 66;
pub const P_H: u8 = 67;

const UNIT_TABLE: [(u32, u8); 72] = [
    (hash_str("c"), U_TEMP_C),
    (hash_bytes(&[0xC2, 0xB0, b'c']), U_TEMP_C),
    (hash_str("celsius"), U_TEMP_C),
    (hash_str("degc"), U_TEMP_C),
    (hash_str("f"), U_TEMP_F),
    (hash_bytes(&[0xC2, 0xB0, b'f']), U_TEMP_F),
    (hash_str("fahrenheit"), U_TEMP_F),
    (hash_str("ms"), U_MS),
    (hash_str("mps"), U_MS),
    (hash_str("m/s"), U_MS),
    (hash_str("kmh"), U_KMH),
    (hash_str("kph"), U_KMH),
    (hash_str("km/h"), U_KMH),
    (hash_str("kt"), U_KT),
    (hash_str("kts"), U_KT),
    (hash_str("knot"), U_KT),
    (hash_str("knots"), U_KT),
    (hash_str("mph"), U_MPH),
    (hash_str("mi/h"), U_MPH),
    (hash_str("percent"), U_PCT),
    (hash_str("pct"), U_PCT),
    (hash_bytes(&[0xC2, 0xB0]), U_DEG),
    (hash_str("deg"), U_DEG),
    (hash_str("degree"), U_DEG),
    (hash_str("degrees"), U_DEG),
    // Hemisphere-suffixed coordinates: `37.75°N`, `104.8669°W`.
    (hash_bytes(&[0xC2, 0xB0, b'n']), U_DEG),
    (hash_bytes(&[0xC2, 0xB0, b's']), U_DEG),
    (hash_bytes(&[0xC2, 0xB0, b'e']), U_DEG),
    (hash_bytes(&[0xC2, 0xB0, b'w']), U_DEG),
    (hash_str("km"), P_KM),
    (hash_str("h"), P_H),
    (hash_str("mm"), U_MM),
    (hash_str("cm"), U_CM),
    (hash_str("inch"), U_IN),
    (hash_str("inches"), U_IN),
    (hash_str("ft"), U_FT),
    (hash_str("feet"), U_FT),
    (hash_str("sec"), U_SEC),
    (hash_str("secs"), U_SEC),
    (hash_str("second"), U_SEC),
    (hash_str("seconds"), U_SEC),
    (hash_str("min"), U_MIN),
    (hash_str("mins"), U_MIN),
    (hash_str("minute"), U_MIN),
    (hash_str("minutes"), U_MIN),
    (hash_str("hr"), U_HOUR),
    (hash_str("hrs"), U_HOUR),
    (hash_str("hour"), U_HOUR),
    (hash_str("hours"), U_HOUR),
    (hash_str("day"), U_DAY),
    (hash_str("days"), U_DAY),
    // Units we do not otherwise need, carried so that stating one is read as the
    // category error it is rather than falling through to "unitless" and
    // free-matching a figure in some other dimension (adversarial review C6).
    (hash_str("k"), U_TEMP_K),
    (hash_str("kelvin"), U_TEMP_K),
    (hash_str("hpa"), U_HPA),
    (hash_str("mb"), U_MB),
    (hash_str("mbar"), U_MB),
    (hash_str("millibar"), U_MB),
    (hash_str("millibars"), U_MB),
    (hash_str("inhg"), U_INHG),
    (hash_str("psi"), U_PSI),
    (hash_str("bar"), U_BAR),
    (hash_str("mi"), U_MI),
    (hash_str("mile"), U_MI),
    (hash_str("miles"), U_MI),
    (hash_str("kg"), U_KG),
    (hash_str("kgs"), U_KG),
    (hash_str("kilogram"), U_KG),
    (hash_str("kilograms"), U_KG),
    (hash_str("lb"), U_LB),
    (hash_str("lbs"), U_LB),
    (hash_str("pound"), U_LB),
    (hash_str("pounds"), U_LB),
];

/// Extra partials that collide with very common words, kept out of the main
/// table so `m` and `s` are only ever read as units directly after a figure.
const PARTIAL_TABLE: [(u32, u8); 2] = [(hash_str("m"), P_M), (hash_str("s"), P_S)];

/// Unit (or partial unit) named by this byte run, if it names one at all.
pub fn unit_of(s: &[u8]) -> Option<u8> {
    if s.is_empty() {
        return None;
    }
    let h = hash_bytes(s);
    let mut i = 0usize;
    while i < UNIT_TABLE.len() {
        if UNIT_TABLE[i].0 == h {
            return Some(UNIT_TABLE[i].1);
        }
        i += 1;
    }
    let mut j = 0usize;
    while j < PARTIAL_TABLE.len() {
        if PARTIAL_TABLE[j].0 == h {
            return Some(PARTIAL_TABLE[j].1);
        }
        j += 1;
    }
    None
}

/// Unit code carried by a whole token, precomputed at tokenise time so the
/// second pass never needs the original bytes back.
pub fn unit_word_code(s: &[u8]) -> u8 {
    match unit_of(s) {
        Some(u) => u,
        None => U_NONE,
    }
}

/// A `°S` / `°W` suffix makes the coordinate negative.
pub fn suffix_is_negative_hemisphere(rest: &[u8]) -> bool {
    if rest.len() < 2 {
        return false;
    }
    let last = lower(rest[rest.len() - 1]);
    (last == b's' || last == b'w') && rest[0] == 0xC2
}

/// Parse the leading decimal run of a token. Returns the value and how many
/// bytes it consumed; `,` is accepted as a thousands separator between digits
/// and at most one `.` is taken as the decimal point.
/// Parse a hyphenated range as a range, not as its floor.
///
/// The tokeniser absorbs the `-` in `5-50`, so before this existed the token
/// carried `val = 5.0` and `5-50 m/s` scored identically to `5 m/s` — every
/// range answer was silently read as its lower bound (adversarial review M6).
/// Returns `(lo, hi, used)`; `hi == lo` when the token is a plain figure.
pub fn leading_range(tok: &[u8]) -> (f32, f32, usize) {
    let (lo, used) = leading_number(tok);
    if used == 0 || used >= tok.len() {
        return (lo, lo, used);
    }
    // `5-50`, `40-60`. A second `-` would make it an identifier (a date), which
    // the classifier has already routed elsewhere.
    if tok[used] == b'-' && used + 1 < tok.len() && is_digit(tok[used + 1]) {
        let (hi, used2) = leading_number(&tok[used + 1..]);
        if used2 > 0 && hi >= lo {
            return (lo, hi, used + 1 + used2);
        }
    }
    (lo, lo, used)
}

pub fn leading_number(tok: &[u8]) -> (f32, usize) {
    let n = tok.len();
    let mut i = 0usize;
    let mut int_part: f32 = 0.0;
    let mut seen_digit = false;
    while i < n {
        let b = tok[i];
        if is_digit(b) {
            int_part = int_part * 10.0 + (b - b'0') as f32;
            seen_digit = true;
            i += 1;
        } else if b == b',' && seen_digit && i + 1 < n && is_digit(tok[i + 1]) {
            i += 1;
        } else {
            break;
        }
    }
    if !seen_digit {
        return (0.0, 0);
    }
    // Optional single fractional part.
    if i + 1 < n && tok[i] == b'.' && is_digit(tok[i + 1]) {
        let mut j = i + 1;
        let mut frac: f32 = 0.0;
        let mut scale: f32 = 1.0;
        while j < n && is_digit(tok[j]) {
            frac = frac * 10.0 + (tok[j] - b'0') as f32;
            scale *= 10.0;
            j += 1;
        }
        // Only a genuine terminator makes this a decimal; `2.14.1` is a version.
        if j >= n || !is_sep(tok[j]) {
            return (int_part + frac / scale, j);
        }
    }
    (int_part, i)
}

/// Second pass over the token stream: attach units that live in the *next*
/// token(s) rather than in the figure itself (`5.9 km/h` tokenises as
/// `5.9`,`km`,`h`), and read a trailing `%` off the source byte after the token.
/// A lone `n`/`s`/`e`/`w` token — the plain-text hemisphere marker.
fn hemisphere_letter(h: u32) -> bool {
    h == hash_str("n") || h == hash_str("s") || h == hash_str("e") || h == hash_str("w")
}

/// Could this figure be a latitude or longitude? Coordinates are fractional and
/// bounded; a duration in seconds or a count is neither. This is what keeps
/// `30 s` a duration while `34.9011 S` is a southern latitude.
fn coordinate_shaped(v: f32) -> bool {
    let a = fabs(v);
    a <= 180.0 && a != ((a as i32) as f32)
}

pub fn annotate_units(t: &mut Toks) {
    let mut k = 0usize;
    while k < t.n {
        if t.kind[k] != K_NUMBER {
            k += 1;
            continue;
        }
        // A lone hemisphere letter is read BEFORE the unit-word table, because
        // three of the four letters name units: `s` is seconds, `n` and `e` are
        // SI prefixes. Only `w` was not in the table, which is why `56.1645 W`
        // was understood while `34.9011 S`, `... N` and `... E` were silently
        // read as times and quantities — a correct southern latitude scored
        // 0.0000 against a ground truth of `-34.9011`, the same as the wrong
        // hemisphere (the attached form `34.9011S` was fine, so no test caught
        // it). The guard is what keeps `30 s` a duration: a hemisphere marker
        // follows a *fractional* magnitude within coordinate range.
        let hemisphere = k + 1 < t.n
            && (t.unit[k] == U_DEG || t.unit[k] == U_NONE)
            && coordinate_shaped(t.val[k])
            && hemisphere_letter(t.hash[k + 1]);
        if hemisphere {
            t.unit[k] = U_DEG;
            let h = t.hash[k + 1];
            if (h == hash_str("s") || h == hash_str("w")) && t.val[k] > 0.0 {
                t.val[k] = -t.val[k];
                t.vhi[k] = t.val[k];
            }
            // The letter is notation belonging to the figure, not a word the
            // answer asserts. Left in the content pool it read as unsupported
            // prose, and the two spelling of the same coordinate differed by
            // 0.052 -- just outside the 0.05 equivalence tolerance.
            t.boiler[k + 1] = true;
        }
        if t.unit[k] == U_NONE {
            if t.nb[k] == b'%' {
                t.unit[k] = U_PCT;
            } else if k + 1 < t.n {
                let u1 = t.uword[k + 1];
                if u1 != U_NONE && u1 < P_BASE {
                    t.unit[k] = u1;
                } else if u1 >= P_BASE {
                    // "km" + "h", "m" + "s": a unit split by a `/` the tokeniser
                    // did not absorb because no digit touched it. Unjoined, the
                    // partial still names a unit of its own — `km` is a distance
                    // and `h` is an hour.
                    let u2 = if k + 2 < t.n { t.uword[k + 2] } else { U_NONE };
                    t.unit[k] = match (u1, u2) {
                        (P_KM, P_H) => U_KMH,
                        (P_M, P_S) => U_MS,
                        (P_KM, _) => U_KM,
                        (P_M, _) => U_M,
                        (P_H, _) => U_HOUR,
                        (P_S, _) => U_SEC,
                        _ => U_NONE,
                    };
                }
            }
        }
        // A figure whose neighbouring word looks like a unit we do not know.
        // Recorded so it can be read as the category error it is, instead of
        // silently becoming a unitless figure that free-matches any dimension.
        if t.unit[k] == U_NONE && k + 1 < t.n {
            let nxt = k + 1;
            // A unit trails its figure and is not itself followed by another
            // figure. Without that second test, `wind_kmh=128.7 gust_kmh=175`
            // reads `gust` as the unit of 128.7 and scores a wholly correct
            // key=value answer as a category error.
            let followed_by_figure = (k + 2 < t.n && t.kind[k + 2] == K_NUMBER)
                || (k + 3 < t.n && t.kind[k + 3] == K_NUMBER);
            // A unit abuts its figure: "47 km/h", "47bananas". Punctuation
            // between them means the word begins a new clause and is not a unit
            // at all. Without this test every enumerated list poisoned its own
            // markers -- "2. Using tools" read as "2 <Using>", a figure in an
            // unknown category, so a correct answer that wrote "2. tools"
            // instead disagreed with the ground truth on a made-up dimension and
            // the whole numeric channel collapsed to 0.145 (measured, CLEAN-PAIR
            // fixture ip_geolocation-cleanpair-11: a faithful terse answer
            // scored 0.5241). Verbatim copies never showed it because the
            // exact-match short-circuit returns before this runs.
            let abuts = t.nb[k] == b' ' || is_alpha(t.nb[k]);
            if abuts
                && t.kind[nxt] == K_WORD
                && t.uword[nxt] == U_NONE
                && t.w[nxt] > 0.1
                && !followed_by_figure
            {
                // Stem, not the raw hash. A unit-shaped word is the same unit
                // however it is inflected: an answer saying "7 matches" against
                // a ground truth saying "7 matching passages" is stating the
                // same quantity, but comparing raw hashes made them foreign to
                // each other and fired `m_foreign_unit` on a CORRECT answer.
                // Measured on CONTENT_VERIFICATION clean pairs: the terse
                // correct phrasing scored fact 0.3941 against 1.0000 for the
                // verbatim one, dragging it to 0.3298 — below a wrong-similarity
                // answer at 0.4921, an inversion.
                t.ufword[k] = t.family[nxt];
            }
        }
        k += 1;
    }
}

//! Fact agreement: does the answer assert entities and figures the ground
//! truth actually supports?
//!
//! The embedding channels measure whether an answer is *about* the right
//! thing. They do not measure whether it is *right*. Measured on real
//! Telegraph traffic: a ground truth reading "Location: Likely Ashburn,
//! Virginia" against an answer saying "San Jose, California" scored 0.9920
//! live, because the two sentences are near-identical in embedding space.
//! Cosine similarity cannot see a swapped city.
//!
//! This module supplies the missing signal as a multiplier. It is deliberately
//! small and lexical: no allocation, no model, a single pass over each string.
//!
//! The guard against over-punishing is the same pairing rule the standalone
//! scorer uses: an unsupported claim only counts against the answer to the
//! extent the ground truth makes claims the answer never mentions. An answer
//! that covers everything the truth says and adds true detail has displaced
//! nothing, so extra detail stays neutral.

const MAX_CLAIMS: usize = 96;
/// How far a disagreement may pull the composite down.
const CHANNEL_W: f32 = 0.92;
/// Floor, so a wholly-unsupported answer degrades rather than zeroing: the
/// embedding channels still carry their own verdict.
const FLOOR: f32 = 0.04;

#[inline]
fn is_digit(b: u8) -> bool {
    b.is_ascii_digit()
}

#[inline]
fn is_alpha(b: u8) -> bool {
    b.is_ascii_alphabetic()
}

#[inline]
fn is_wordbyte(b: u8) -> bool {
    is_alpha(b) || is_digit(b) || b == b'.' || b == b'-' || b == b'_'
}

/// FNV-1a over the case-folded token.
fn hash_lower(bytes: &[u8]) -> u32 {
    let mut h: u32 = 0x811c_9dc5;
    let mut i = 0usize;
    while i < bytes.len() {
        let c = bytes[i].to_ascii_lowercase();
        h ^= c as u32;
        h = h.wrapping_mul(0x0100_0193);
        i += 1;
    }
    h
}

/// A token is a *claim* when it decides the answer: a capitalised word (an
/// entity) or anything carrying a digit (a figure or identifier). Ordinary
/// lowercase prose is left to the embedding channels.
fn is_claim(tok: &[u8], sentence_start: bool) -> bool {
    if tok.len() < 2 {
        return false;
    }
    // A bare figure is deliberately NOT a claim here. This channel matches
    // lexically, and a figure only means anything after unit normalisation:
    // 18 km/h and 5 m/s are the same speed but share no token, so treating
    // them as claims made a correct answer in other units score below a wrong
    // one. Figures are left to the embedding channels; what this channel adds
    // is entities and identifiers, which are exactly what cosine similarity
    // cannot distinguish.
    let mut digits = 0usize;
    let mut alpha = false;
    let mut i = 0usize;
    while i < tok.len() {
        if is_digit(tok[i]) {
            digits += 1;
        } else if is_alpha(tok[i]) {
            alpha = true;
        }
        i += 1;
    }
    // An identifier: letters mixed with digits (AS15169, CVE-2024-1234) or a
    // dotted numeric run (8.8.8.8). Not a plain number.
    if digits >= 2 && (alpha || tok.contains(&b'.') || tok.contains(&b'-')) {
        return true;
    }
    if digits > 0 {
        return false;
    }
    // A capitalised word mid-sentence names something. At a sentence start the
    // capital is grammatical, so it says nothing.
    !sentence_start && tok[0].is_ascii_uppercase()
}

/// Collect claim hashes from `text` into `out`, returning how many were found.
fn collect(text: &str, out: &mut [u32; MAX_CLAIMS]) -> usize {
    let src = text.as_bytes();
    let n = src.len();
    let mut count = 0usize;
    let mut i = 0usize;
    let mut sentence_start = true;
    while i < n {
        if !is_wordbyte(src[i]) {
            // A terminator means the next word's capital is grammatical.
            if src[i] == b'.' || src[i] == b'!' || src[i] == b'?' || src[i] == b'\n' {
                sentence_start = true;
            }
            i += 1;
            continue;
        }
        let start = i;
        while i < n && is_wordbyte(src[i]) {
            i += 1;
        }
        let tok = &src[start..i];
        if is_claim(tok, sentence_start) && count < MAX_CLAIMS {
            out[count] = hash_lower(tok);
            count += 1;
        }
        sentence_start = false;
    }
    count
}

fn contains(set: &[u32; MAX_CLAIMS], len: usize, h: u32) -> bool {
    let mut i = 0usize;
    while i < len {
        if set[i] == h {
            return true;
        }
        i += 1;
    }
    false
}

/// Multiplier in `[FLOOR, 1.0]` for how well the answer's claims are supported.
///
/// Returns exactly 1.0 when neither side makes checkable claims, so a purely
/// prose answer is judged by the embedding channels alone rather than being
/// dragged down by a channel that has nothing to say.
pub fn agreement(question: &str, ground_truth: &str, answer: &str) -> f32 {
    let mut gt = [0u32; MAX_CLAIMS];
    let mut ans_all = [0u32; MAX_CLAIMS];
    let mut q = [0u32; MAX_CLAIMS];
    let gt_n = collect(ground_truth, &mut gt);
    let ans_all_n = collect(answer, &mut ans_all);
    let q_n = collect(question, &mut q);

    // A claim the question already made is an echo, not an assertion. Without
    // this a refusal reading "I cannot determine the location of this IP
    // address" counts `IP` as supported content and escapes the channel
    // entirely, which is how it outscored a wrong answer.
    let mut ans = [0u32; MAX_CLAIMS];
    let mut ans_n = 0usize;
    let mut j = 0usize;
    while j < ans_all_n {
        if !contains(&q, q_n, ans_all[j]) {
            ans[ans_n] = ans_all[j];
            ans_n += 1;
        }
        j += 1;
    }
    if gt_n == 0 {
        // The truth makes no checkable claim, so this channel has nothing to
        // say and defers to the embedding signals entirely.
        return 1.0;
    }
    if ans_n == 0 {
        // The truth names entities and figures and the answer names none. A
        // refusal or a content-free restatement lands here: it has not
        // answered, and the embedding channels score it around 0.51 purely
        // for being topical prose.
        return FLOOR;
    }

    let mut supported = 0usize;
    let mut unsupported = 0usize;
    let mut i = 0usize;
    while i < ans_n {
        if contains(&gt, gt_n, ans[i]) {
            supported += 1;
        } else {
            unsupported += 1;
        }
        i += 1;
    }

    // Ground-truth claims the answer never mentions. This is what separates a
    // substitution from an addition: with nothing uncovered, extra detail in
    // the answer displaced nothing and is not charged.
    let mut uncovered = 0usize;
    let mut k = 0usize;
    while k < gt_n {
        if !contains(&ans, ans_n, gt[k]) && !contains(&q, q_n, gt[k]) {
            uncovered += 1;
        }
        k += 1;
    }

    let substituted = if unsupported < uncovered {
        unsupported
    } else {
        uncovered
    };
    if substituted == 0 {
        return 1.0;
    }

    let denom = (supported + substituted) as f32;
    let agree = if denom > 0.0 {
        supported as f32 / denom
    } else {
        0.0
    };
    let scaled = 1.0 - CHANNEL_W * (1.0 - agree);
    let clamped = if scaled < 0.0 {
        0.0
    } else if scaled > 1.0 {
        1.0
    } else {
        scaled
    };
    FLOOR + (1.0 - FLOOR) * clamped
}

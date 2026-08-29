//! Polar verdict terms: word pairs that assert opposite findings.
//!
//! Why this exists. `polarity_of` catches a *negation* — "is located" against
//! "is **not** located" — because the two share a token and differ in `neg`.
//! It cannot catch an **antonym**, where the flip is carried by a different word
//! entirely. On any intent whose answer is a one-word verdict that is the whole
//! finding, and the miss is nearly free: a verdict term is a lowercase common
//! word, so it is neither an entity nor a figure and falls through to prose,
//! which carries `prose_w = 0.02`.
//!
//! Measured on CONTENT_VERIFICATION clean pairs before this table: flipping
//! "plagiarised" to "original" and changing nothing else scored **0.9999**
//! against a verbatim-correct 1.0000 — the exact inversion class this project
//! criticises the incumbent for, in our own module.
//!
//! Scope, deliberately narrow. These are general English polar pairs used as
//! verdicts across many intents (authenticity, validity, safety, liveness), not
//! a fixture list and not a fact about any miner: the table would be the same if
//! every hidden benchmark were replaced tomorrow, which is the legitimacy test
//! in A4. Comparatives, hedges and domain jargon are out of scope — a term that
//! is merely *different* is already handled as an unsupported token.

use crate::fs::bytes::hash_str;

/// Case-folded FNV-1a hashes of polar pairs. Order within a pair is irrelevant;
/// the lookup tests both directions.
static AXES: [(u32, u32); 28] = [
    (hash_str("plagiarised"), hash_str("original")),
    (hash_str("plagiarized"), hash_str("original")),
    (hash_str("plagiarised"), hash_str("authentic")),
    (hash_str("plagiarized"), hash_str("authentic")),
    (hash_str("copied"), hash_str("original")),
    (hash_str("duplicate"), hash_str("original")),
    (hash_str("fake"), hash_str("genuine")),
    (hash_str("fake"), hash_str("authentic")),
    (hash_str("forged"), hash_str("genuine")),
    (hash_str("fabricated"), hash_str("genuine")),
    (hash_str("valid"), hash_str("invalid")),
    (hash_str("verified"), hash_str("unverified")),
    (hash_str("trusted"), hash_str("untrusted")),
    (hash_str("safe"), hash_str("malicious")),
    (hash_str("safe"), hash_str("unsafe")),
    (hash_str("clean"), hash_str("infected")),
    (hash_str("expired"), hash_str("current")),
    (hash_str("revoked"), hash_str("active")),
    (hash_str("reachable"), hash_str("unreachable")),
    (hash_str("online"), hash_str("offline")),
    (hash_str("true"), hash_str("false")),
    (hash_str("accurate"), hash_str("inaccurate")),
    (hash_str("correct"), hash_str("incorrect")),
    (hash_str("consistent"), hash_str("inconsistent")),
    // Hyphenated verdicts tokenise apart, so the axis must exist at the
    // component level too: "AI-generated" against "human-written" shares no
    // whole-token pair and scored 0.9999 until these were added.
    (hash_str("generated"), hash_str("written")),
    (hash_str("ai"), hash_str("human")),
    (hash_str("machine"), hash_str("human")),
    (hash_str("synthetic"), hash_str("human")),
];

/// True when `a` and `b` are opposite verdicts on the same axis.
pub fn opposes(a: u32, b: u32) -> bool {
    let mut i = 0usize;
    while i < AXES.len() {
        let (x, y) = AXES[i];
        if (a == x && b == y) || (a == y && b == x) {
            return true;
        }
        i += 1;
    }
    false
}

/// True when `h` names a verdict on any axis — used to decide whether an
/// unsupported token is a *claim* worth checking against the ground truth.
pub fn is_verdict(h: u32) -> bool {
    let mut i = 0usize;
    while i < AXES.len() {
        if AXES[i].0 == h || AXES[i].1 == h {
            return true;
        }
        i += 1;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::bytes::hash_str;

    #[test]
    fn opposites_are_symmetric() {
        assert!(opposes(hash_str("plagiarised"), hash_str("original")));
        assert!(opposes(hash_str("original"), hash_str("plagiarised")));
        assert!(opposes(hash_str("valid"), hash_str("invalid")));
    }

    #[test]
    fn unrelated_words_do_not_oppose() {
        assert!(!opposes(hash_str("plagiarised"), hash_str("tokyo")));
        assert!(!opposes(hash_str("original"), hash_str("original")));
        assert!(!opposes(hash_str("google"), hash_str("cloudflare")));
    }

    #[test]
    fn verdict_membership() {
        assert!(is_verdict(hash_str("authentic")));
        assert!(!is_verdict(hash_str("mumbai")));
    }
}

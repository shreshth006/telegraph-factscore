//! Open-addressed hash set over token keys, so matching an answer against the
//! ground truth is linear rather than n*m. Fixed capacity, no allocator.

#![allow(dead_code)]

use crate::fs::tokens::Toks;

pub const SET_SLOTS: usize = 8192;

pub struct Set {
    key: [u32; SET_SLOTS],
    /// Token index + 1. Zero means the slot is free, which is what lets `fill`
    /// reset the whole set by clearing one array instead of two.
    val: [u32; SET_SLOTS],
}

// Deliberately a `const`, not a `static`: it is an initializer copied into a
// local at each use, so there is no shared mutable state and no relocation.
// A `static` would force a borrow and defeat that.
#[allow(clippy::large_const_arrays)]
pub const EMPTY_SET: Set = Set {
    key: [0; SET_SLOTS],
    val: [0; SET_SLOTS],
};

impl Set {
    fn insert(&mut self, key: u32, idx: usize) {
        let mut slot = (key as usize) & (SET_SLOTS - 1);
        let mut probes = 0usize;
        while probes < SET_SLOTS {
            if self.val[slot] == 0 {
                self.key[slot] = key;
                self.val[slot] = idx as u32 + 1;
                return;
            }
            if self.key[slot] == key {
                // Keep the first occurrence: earlier tokens are likelier to be
                // the subject rather than a later restatement.
                return;
            }
            slot = (slot + 1) & (SET_SLOTS - 1);
            probes += 1;
        }
    }

    pub fn get(&self, key: u32) -> Option<usize> {
        let mut slot = (key as usize) & (SET_SLOTS - 1);
        let mut probes = 0usize;
        while probes < SET_SLOTS {
            if self.val[slot] == 0 {
                return None;
            }
            if self.key[slot] == key {
                return Some((self.val[slot] - 1) as usize);
            }
            slot = (slot + 1) & (SET_SLOTS - 1);
            probes += 1;
        }
        None
    }

    pub fn has(&self, key: u32) -> bool {
        self.get(key).is_some()
    }

    /// Reset and load every token of `t` under both its exact and stemmed key.
    pub fn fill(&mut self, t: &Toks) {
        let mut i = 0usize;
        while i < SET_SLOTS {
            self.val[i] = 0;
            i += 1;
        }
        let mut k = 0usize;
        while k < t.n {
            self.insert(t.hash[k], k);
            if t.stem[k] != t.hash[k] {
                self.insert(t.stem[k], k);
            }
            k += 1;
        }
    }

    /// Add a key that is not a token of its own — used for the acronym of a run
    /// of proper nouns, so an answer saying "US" finds a ground truth that says
    /// "United States" without a hard-coded synonym table.
    pub fn insert_key(&mut self, key: u32, idx: usize) {
        self.insert(key, idx);
    }

    /// Where token `i` of `t` occurs in this set, by exact form or by stem.
    pub fn find(&self, t: &Toks, i: usize) -> Option<usize> {
        if let Some(k) = self.get(t.hash[i]) {
            return Some(k);
        }
        self.get(t.stem[i])
    }

    pub fn contains_tok(&self, t: &Toks, i: usize) -> bool {
        self.find(t, i).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::tokens::tokenize;

    #[test]
    fn set_round_trips_tokens_and_stems() {
        let mut t = Toks::new();
        tokenize(b"Google LLC provides hosting", &mut t);
        let mut s = EMPTY_SET;
        s.fill(&t);
        let mut a = Toks::new();
        tokenize(b"google provide", &mut a);
        // exact form
        assert!(s.contains_tok(&a, 0));
        // stemmed form: "provide" must find "provides"
        assert!(s.contains_tok(&a, 1));
        let mut miss = Toks::new();
        tokenize(b"cloudflare", &mut miss);
        assert!(!s.contains_tok(&miss, 0));
    }

    #[test]
    fn fill_clears_previous_contents() {
        let mut s = EMPTY_SET;
        let mut t = Toks::new();
        tokenize(b"alpha", &mut t);
        s.fill(&t);
        tokenize(b"beta", &mut t);
        s.fill(&t);
        let mut probe = Toks::new();
        tokenize(b"alpha", &mut probe);
        // A stale key surviving a refill would make the score depend on call
        // order, which is the one thing a scorer must never do.
        assert!(!s.contains_tok(&probe, 0));
    }
}

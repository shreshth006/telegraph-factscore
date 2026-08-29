//! The scoring pipeline.
//!
//! Precision of the answer, gated by answered-ness, multiplied by typed fact
//! agreement, calibrated with a smoothstep. In order:
//!
//!   1. tokenise question / ground truth / answer;
//!   2. strike the `"The data ..."` boilerplate opener from the answer;
//!   3. mark each answer token as echoed (present in the question) and/or
//!      supported (present in the ground truth, with numeric tolerance);
//!   4. **P** — weighted fraction of what the answer *asserts* that the ground
//!      truth supports, measured over decisive facts and prose separately;
//!   5. **ans** — answered-ness: does the answer carry novel supported content
//!      at all? A question-echo and a content-filter refusal both fail here;
//!   6. **fmul** — typed fact agreement, multiplicative (A3.4);
//!   7. smoothstep calibration for genuine spread, never a cliff (A3.7).

use crate::aliases::{
    code_for_name, is_corporate_suffix, is_place_code, name_for_code, state_code_for_name,
    state_name_for_code,
};
use crate::bytes::*;
use crate::facts::{best_agreement, fact_multiplier};
use crate::profile::{profile, Profile};
use crate::sets::{Set, EMPTY_SET};
use crate::tokens::{mark_boilerplate, tokenize, Toks, EMPTY_TOKS, K_NUMBER};
use crate::units::annotate_units;

// Scratch state. The module is single-threaded and the host gives each call a
// fresh logical invocation, so these are reset at the top of every score().
static mut TQ: Toks = EMPTY_TOKS;
static mut TG: Toks = EMPTY_TOKS;
static mut TA: Toks = EMPTY_TOKS;
static mut SQ: Set = EMPTY_SET;
static mut SG: Set = EMPTY_SET;
static mut SA: Set = EMPTY_SET;

/// Scratch for `fold_punct`. Three buffers, one per text, because `Toks` keeps
/// no reference to the source but the three tokenise calls read their slices in
/// turn. Sized for the longest real text by a wide margin — a live
/// `converted_answer` runs a few hundred bytes and the longest ground truth in
/// the corpus is ~2 KB. A text beyond this is an adversarial Stage-1 probe and
/// is tokenised raw, where the only requirement is not to trap.
const FOLD_CAP: usize = 8192;
static mut FOLDED: [[u8; FOLD_CAP]; 3] = [[0; FOLD_CAP]; 3];

/// The shipped module is single-threaded wasm, where these statics are simply
/// scratch. `cargo test` on the host runs tests in parallel threads against the
/// same statics, so host test builds serialise entry. This exists only so the
/// tests are meaningful; it compiles out of every wasm build.
#[cfg(all(test, not(target_arch = "wasm32")))]
static SCRATCH_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// The five debug figures behind a score, in the order `breakdown_answer` writes
/// them: precision, fact agreement, answered-ness, raw composite, final score.
#[derive(Clone, Copy)]
pub struct Breakdown {
    pub precision: f32,
    pub fact: f32,
    pub answered: f32,
    pub raw: f32,
    pub final_score: f32,
}

/// What an answer keeps when it asserts nothing the question did not already
/// contain. A pure echo is not an answer, and scoring it as one is how a
/// contentless parrot came to outrank an answer carrying real data.
///
/// Swept against the 39-fixture corpus: 0.01 through 0.12 all hold 46 of 47
/// pairwise wins, so this is set for headroom rather than at the edge.
const NO_NOVELTY_KEEP: f32 = 0.05;

/// How far an answer carrying real content is lifted off an exact zero, so it
/// cannot rank below one that asserts nothing. Small enough to be irrelevant
/// wherever the other channels have already separated the pair.
///
/// The node rejected this module on ordering, not separation, so the sweep
/// resolved in favour of ordering headroom: 0.001 buys 0.0013 more margin and
/// four times less room above a parrot that lands higher than the ones in this
/// corpus. Every value from 0.001 to 0.004 holds 46 of 47 and beats the
/// baseline margin of 0.8627.
const FLOOR_LIFT: f32 = 0.004;

/// Decisive tokens the answer asserts that the question did not already carry.
///
/// A pure question echo has none by construction. Any answer carrying real
/// data has several, whether or not those data are correct -- which is exactly
/// the distinction the tie-break needs, and the one answeredness cannot make.
///
/// Saturating at one: the distinction this draws is whether the answer asserts
/// anything the question did not already carry, not how much. Requiring two
/// halved correct short answers -- "The window is 30 seconds." contributes a
/// single novel token and was scored 0.525 for it.
fn novel_assertions(ta: &Toks, sq: &Set) -> f32 {
    let mut n = 0usize;
    let mut i = 0usize;
    while i < ta.n {
        if ta.decisive[i] && !sq.contains_tok(ta, i) {
            n += 1;
        }
        i += 1;
    }
    let f = n as f32;
    if f > 1.0 {
        1.0
    } else {
        f
    }
}

pub fn score(q: &[u8], gt: &[u8], ma: &[u8]) -> f32 {
    breakdown(q, gt, ma).final_score
}

pub fn breakdown(q: &[u8], gt: &[u8], ma: &[u8]) -> Breakdown {
    #[cfg(all(test, not(target_arch = "wasm32")))]
    let _scratch = SCRATCH_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let p = profile();
    let zero = Breakdown {
        precision: 0.0,
        fact: 0.0,
        answered: 0.0,
        raw: 0.0,
        final_score: 0.0,
    };

    // Nothing to support the answer against: abstain at zero rather than invent
    // a score from the question alone.
    if gt.is_empty() || is_blank(ma) {
        return zero;
    }

    // SAFETY: single-threaded wasm; the three token buffers and two sets are
    // distinct statics, and every field is rewritten before it is read.
    let (tq, tg, ta, sq, sg, sa) = unsafe {
        (
            &mut *core::ptr::addr_of_mut!(TQ),
            &mut *core::ptr::addr_of_mut!(TG),
            &mut *core::ptr::addr_of_mut!(TA),
            &mut *core::ptr::addr_of_mut!(SQ),
            &mut *core::ptr::addr_of_mut!(SG),
            &mut *core::ptr::addr_of_mut!(SA),
        )
    };

    // Fold Unicode punctuation to ASCII before tokenising. Bytes >= 0x80 are
    // opaque word bytes, so a curly apostrophe glues its neighbours into one
    // token where the ASCII one splits them, and a correct answer failed on
    // typography alone: against a ground truth reading `Shimo\u{2019}ochiai`,
    // the same answer written with `'` scored 0.2592 and the curly form 0.9997.
    // SAFETY: single-threaded wasm (host test builds serialise on SCRATCH_LOCK);
    // the three fold slots are distinct statics and each is written before read.
    let (fq, fg, fa) = unsafe {
        let all = &mut *core::ptr::addr_of_mut!(FOLDED);
        let (a, rest) = all.split_at_mut(1);
        let (b, c) = rest.split_at_mut(1);
        (&mut a[0], &mut b[0], &mut c[0])
    };
    tokenize(fold(q, fq), tq);
    tokenize(fold(gt, fg), tg);
    tokenize(fold(ma, fa), ta);
    annotate_units(tq);
    annotate_units(tg);
    annotate_units(ta);
    if ta.n == 0 || tg.n == 0 {
        return zero;
    }

    sq.fill(tq);
    sg.fill(tg);
    sa.fill(ta);
    // A miner may legitimately write "US" where the ground truth writes "United
    // States". Reducing each run of proper nouns to its initials and adding that
    // as a key covers the general case without a synonym table (which would be a
    // phrasing match, and Rule-04 says we do not do those).
    add_acronyms(sg, tg);
    add_acronyms(sa, ta);
    // The initials rule cannot reach a single-word country, so ISO 3166 alpha-2
    // codes are indexed explicitly, in both directions.
    add_country_aliases(sg, tg);
    add_country_aliases(sa, ta);
    mark_boilerplate(ta);

    // Mark echo and support. Support is **graded**, not boolean: collapsing the
    // agreement to `>= 1 - 1e-6` put a hard cliff on top of a smooth curve, and
    // a 1% change in an asserted figure moved the score by 0.999 whenever the
    // figure was the answer's only decisive content (adversarial review M2).
    let mut i = 0usize;
    while i < ta.n {
        ta.echo[i] = sq.contains_tok(ta, i);
        if ta.kind[i] == K_NUMBER {
            // A figure is supported to the degree some ground-truth figure
            // agrees with it — not merely when the same digits appear.
            ta.supw[i] = best_agreement(ta, i, tg, &p).unwrap_or(0.0);
            ta.supi[i] = 0;
        } else {
            match sg.find(ta, i) {
                Some(k) => {
                    ta.supw[i] = 1.0;
                    ta.supi[i] = k as u32 + 1;
                }
                None => {
                    ta.supw[i] = 0.0;
                    ta.supi[i] = 0;
                }
            }
        }
        i += 1;
    }

    // Computed once and read by three channels: entity, identifier, precision.
    let gt_uncovered = gt_uncovered_mass(ta, tg, sa);
    let entity = entity_agreement(ta, gt_uncovered, &p);
    let precision = precision_of(ta, gt_uncovered, &p);
    let answered = answeredness(ta, tg, sq, &p);
    let (fmul, fact_raw) = fact_multiplier(ta, tg, sa, &p);
    let polarity = polarity_of(ta, tg, &p);

    // Concave shaping pulls a mostly-right answer up without flattening the
    // middle; p_concave = 0 leaves precision linear.
    let shaped = (1.0 - p.p_concave) * precision + p.p_concave * (precision * (2.0 - precision));

    let raw = clamp01(shaped * fmul * entity * answered * polarity);
    let stepped = clamp01(smoothstep(p.ss_lo, p.ss_hi, raw));

    // Answeredness tie-break.
    //
    // The composite is a product, so one contradicted entity drives the whole
    // score to zero -- below a contentless parrot, which is penalised by no
    // channel because it asserts nothing. Measured on the probe corpus: an
    // answer carrying real data with one wrong city scored 0.0000 while a pure
    // question echo scored 0.0071, and REAL-PARROT was the only fixture class
    // below 100%, at 3 of 8.
    //
    // An answer that engages with the question should not rank below one that
    // does not, however wrong it is. This adds a term that is only ever
    // decisive when the stepped score is already near zero: it is scaled by
    // (1 - stepped), so it vanishes wherever the score carries real signal and
    // cannot reorder the classes that already separate cleanly.
    //
    // The signal is novel assertions, not answeredness. A parrot repeats every
    // dimension the question named, so it reads as maximally responsive; what
    // it never does is assert anything the question did not already contain.
    //
    // An empty answer has answeredness 0, so it still returns exactly 0.0,
    // which the structural gate requires.
    // Demote the contentless rather than lift the wrong. Adding to a bad
    // answer's score fixes the ordering but spends separation, which is the
    // trade that has failed here before: the gate wants both. Scaling an
    // answer that asserts nothing new *down* fixes the same orderings and
    // widens the margin instead of narrowing it.
    //
    // A correct answer necessarily asserts something the question did not
    // already contain, so this never touches one.
    // Two halves of the same correction, and both are needed.
    //
    // Demotion alone cannot fix the ordering: an answer contradicted on its
    // decisive entity scores exactly 0.0, and nothing scales below zero. So the
    // gate pushes the contentless down, and a small additive term lifts
    // anything that asserts real content off the floor. The additive part is
    // scaled by (1 - stepped) so it is spent almost entirely on near-zero
    // scores and leaves separated answers where they are.
    let novel = novel_assertions(ta, sq);
    let gated = stepped * (NO_NOVELTY_KEEP + (1.0 - NO_NOVELTY_KEEP) * novel);
    let final_score = clamp01(gated + FLOOR_LIFT * novel * (1.0 - stepped));

    Breakdown {
        precision,
        fact: fact_raw * polarity * entity,
        answered,
        raw,
        final_score,
    }
}

/// Fold Unicode punctuation into slot `slot`'s scratch buffer and return a view
/// of it. Falls back to the raw bytes when the text does not fit.
fn fold<'a>(src: &'a [u8], buf: &'a mut [u8; FOLD_CAP]) -> &'a [u8] {
    match fold_punct(src, buf) {
        Some(len) => &buf[..len],
        None => src,
    }
}

/// Does this unsupported token abstain rather than count as a wrong entity?
///
/// Only a two-letter ALL-CAPS token we cannot check: `IP`, `AS`, `RIR` name a
/// method or a field, not a value, and their expansion cannot be verified
/// lexically. The exemption used to cover *every* ALL-CAPS token, which let a
/// wrong ISP written "AWS" score 0.9829 against a truth of "Google LLC" while
/// the same swap spelled "Cloudflare Inc." scored 0.2248; bounding it at two
/// letters put organisation acronyms back in the channel.
///
/// A two-letter token that IS a known ISO 3166 code does **not** abstain. It is
/// a checkable claim about a country: `UY` against a truth of Uruguay is support
/// (the alias table matched it), and `DE` against that truth is a wrong entity.
/// Abstaining on those made an appended false country free.
fn abstains(t: &Toks, i: usize) -> bool {
    t.abbrev[i] && !is_place_code(t.hash[i])
}

/// Index each token by its ISO 3166 counterpart, in both directions.
///
/// The acronym rule below turns a *run* of proper nouns into its initials, which
/// is how `US` finds `United States`. It cannot reach a single-word country:
/// nothing lexical turns `Uruguay` into `UY`. Measured before this, a correct
/// paraphrase writing `UY` scored 0.9696 against 1.0000 for the spelled-out
/// form, and — worse — a *wrong* code cost nothing, because unknown two-letter
/// tokens abstained wholesale.
fn add_country_aliases(set: &mut Set, t: &Toks) {
    let mut i = 0usize;
    while i < t.n {
        // Case matters, and getting this wrong is worse than having no table at
        // all. Alpha-2 codes collide with ordinary English words — `is` is
        // Iceland, `it` Italy, `in` India, `no` Norway, `at` Austria. Reading
        // them by hash alone put "iceland" into the ground truth's key set for
        // every sentence containing the verb "is", and a counterfactual that
        // swapped Uruguay for Iceland then scored a perfect **1.0000**.
        //
        // A code is only a code when it is written as one (ALL-CAPS, two
        // letters), and a name is only a name when it is capitalised
        // mid-sentence. Both flags come straight from the tokeniser.
        if t.kind[i] == crate::tokens::K_WORD {
            if t.proper[i] {
                if let Some(code) = code_for_name(t.hash[i]) {
                    set.insert_key(code, i);
                }
                if let Some(code) = state_code_for_name(t.hash[i]) {
                    set.insert_key(code, i);
                }
            }
            if t.abbrev[i] {
                // A code that names both a country and a state indexes both
                // readings, so it is support for whichever one the ground truth
                // contains and contradicts only when it contains neither.
                if let Some(name) = name_for_code(t.hash[i]) {
                    set.insert_key(name, i);
                }
                if let Some(name) = state_name_for_code(t.hash[i]) {
                    set.insert_key(name, i);
                }
            }
        }
        i += 1;
    }
}

/// Add, for each run of consecutive proper nouns, the acronym of its initials.
fn add_acronyms(set: &mut Set, t: &Toks) {
    let mut i = 0usize;
    while i < t.n {
        if !t.proper[i] || t.first[i] == 0 {
            i += 1;
            continue;
        }
        let mut letters = [0u8; 5];
        let mut n = 0usize;
        let mut j = i;
        loop {
            if j >= t.n || !t.proper[j] || t.first[j] == 0 || n >= 5 {
                break;
            }
            letters[n] = t.first[j];
            n += 1;
            // A run is one *phrase*. Without this, "Mountain View, California,
            // United States" reads as one five-token run spelling "mvcus", and
            // the "us" a miner would actually write is never generated.
            if is_phrase_break(t.nb[j]) {
                j += 1;
                break;
            }
            j += 1;
        }
        // Two initials is the shortest useful acronym ("US"); one is just a letter.
        let mut len = 2usize;
        while len <= n {
            set.insert_key(hash_bytes(&letters[..len]), i);
            len += 1;
        }
        i = if j > i { j } else { i + 1 };
    }
}

/// Penalty for asserting a **different entity** than the ground truth does.
///
/// Figures and identifiers go through the typed fact term; proper nouns did not,
/// so a single swapped entity was only diluted precision mass. Measured on the
/// pre-flight repro: swapping one city in an otherwise verbatim answer scored a
/// literal 1.0000, tying the correct answer and *outranking* a correctly-reworded
/// one at 0.9606. A wrong city is not slightly-less-supported prose; it is a
/// contradiction, and it belongs in a multiplicative channel like every other
/// wrong fact.
///
/// The guard against over-punishing is the pairing rule: an unsupported entity
/// only counts as a *substitution* to the extent the ground truth has salient
/// entities of its own that the answer never mentions. An answer that covers
/// everything the truth says and adds something extra has invented nothing to
/// contradict — additional unrequested entities stay neutral, per the precision
/// thesis (A3.8).
/// Ground-truth entity mass the answer never mentions.
///
/// Coverage counts *occurrences*, not mere membership: a ground truth reading
/// "Montevideo, Montevideo, Uruguay" names that entity in two roles, so an answer
/// that says it once has left one of them uncovered. Without this, swapping the
/// city of a city-equals-region record is invisible — the token is still there,
/// just doing the other job.
///
/// This is the quantity that separates a **substitution** from an **addition**,
/// and three channels read it: the entity channel, the identifier channel, and
/// precision. Zero means the answer said everything the truth says, so anything
/// extra it also said has displaced nothing.
fn gt_uncovered_mass(ta: &Toks, tg: &Toks, sa: &Set) -> f32 {
    let mut gt_uncovered = 0.0f32;
    let mut k = 0usize;
    while k < tg.n {
        // Method vocabulary in the ground truth is not entity mass. `abstains`
        // already refuses to charge the answer for writing `IP`, `AS` or `RIR`;
        // the same tokens must not open a substitution budget from the truth's
        // side either. Before this, a truth reading "The IP address ..." against
        // an answer reading "The IPv4 address ..." left `IP` uncovered, so every
        // extra true entity the answer volunteered was paired against it and
        // charged as a substitution: a correct verbose answer scored 0.1872,
        // below the 0.3057 given to one naming the wrong city.
        if tg.proper[k] && !abstains(tg, k) && !is_corporate_suffix(tg.hash[k]) {
            if !sa.contains_tok(tg, k) {
                gt_uncovered += tg.w[k];
            } else {
                let mut prior = 0usize;
                let mut j = 0usize;
                while j < k {
                    if tg.proper[j] && tg.hash[j] == tg.hash[k] {
                        prior += 1;
                    }
                    j += 1;
                }
                if prior > 0 {
                    let mut have = 0usize;
                    let mut a = 0usize;
                    while a < ta.n {
                        if ta.proper[a] && ta.hash[a] == tg.hash[k] {
                            have += 1;
                        }
                        a += 1;
                    }
                    if have <= prior {
                        gt_uncovered += tg.w[k];
                    }
                }
            }
        }
        k += 1;
    }
    gt_uncovered
}

fn entity_agreement(ta: &Toks, gt_uncovered: f32, p: &Profile) -> f32 {
    let (mut supported, mut unsupported) = (0.0f32, 0.0f32);
    let mut i = 0usize;
    while i < ta.n {
        if ta.proper[i] && !ta.boiler[i] && !ta.echo[i] {
            if ta.supw[i] > 0.0 {
                supported += ta.w[i];
            } else if !abstains(ta, i) {
                unsupported += ta.w[i];
            }
        }
        i += 1;
    }

    // Only the paired part is a substitution. The excess is a pure addition: it
    // displaced nothing, so it cannot be a contradiction — but it is not free
    // either. At `add_w = 0` an answer could append an invented city, country or
    // organisation to an otherwise perfect answer for nothing (all measured
    // >= 0.9999). It enters the denominator at a reduced weight, so padding
    // costs, while an answer volunteering *true* extra detail stays near the top.
    let substituted = fmin(unsupported, gt_uncovered);
    let addition = fmax(unsupported - substituted, 0.0);
    let total = supported + substituted + p.add_w * addition;
    if total <= 0.0 {
        return 1.0;
    }
    let mean = supported / total;
    // Worst-case leaning, exactly as the numeric channel is: one swapped entity
    // must not hide behind five correct ones.
    let worst = if substituted > 0.0 { 0.0 } else { 1.0 };
    let agree = (1.0 - p.ent_min_bias) * mean + p.ent_min_bias * worst;
    clamp01(1.0 - p.ent_channel_w * (1.0 - agree))
}

/// Penalty for asserting the opposite of what the ground truth says.
///
/// Support is a set-membership test, so it cannot see polarity: before this, "is
/// located in Germany" and "is **not** located in Germany" differed by one
/// 0.05-weight stopword out of a ~15-token pool and tied at 1.0000 (adversarial
/// review C2). A supported token whose negation state disagrees with the ground
/// truth occurrence it matched is not coverage; it is a contradiction.
fn polarity_of(ta: &Toks, tg: &Toks, p: &Profile) -> f32 {
    let (mut sup_mass, mut contra_mass) = (0.0f32, 0.0f32);
    let mut verdict_flip = false;
    // Antonym pass. A polar verdict carried by a *different word* is invisible
    // to the negation test below -- "plagiarised" against "original" shares no
    // token, so it is merely unsupported, and an unsupported lowercase word is
    // charged at `prose_w`. Measured before this: a flipped verdict scored
    // 0.9999 against a verbatim-correct 1.0000 on CONTENT_VERIFICATION clean
    // pairs. Treat it as what it is -- a contradiction of a stated finding.
    {
        let mut a = 0usize;
        while a < ta.n {
            if !ta.boiler[a] && !ta.echo[a] && crate::antonyms::is_verdict(ta.hash[a]) {
                let mut g = 0usize;
                let (mut stated_same, mut stated_opposite) = (false, false);
                while g < tg.n {
                    if tg.hash[g] == ta.hash[a] {
                        stated_same = true;
                    } else if crate::antonyms::opposes(ta.hash[a], tg.hash[g]) {
                        stated_opposite = true;
                    }
                    g += 1;
                }
                // Only a ground truth that commits to the opposite verdict, and
                // never to this one, is a contradiction. A truth that uses both
                // (discussing the alternative it ruled out) abstains.
                if stated_opposite && !stated_same {
                    // Categorical, not proportional. Routing this through the
                    // mass ratio diluted one flipped word across every other
                    // supported token, so a verdict flip still scored 0.9999 on
                    // a long answer. Asserting the opposite finding is wrong
                    // outright, however much else the answer got right.
                    verdict_flip = true;
                }
            }
            a += 1;
        }
    }
    let mut i = 0usize;
    while i < ta.n {
        // Any supported *content* token can carry a polarity claim — not just a
        // decisive one. The claim in "is not a known proxy and is not flagged
        // for abuse" lives entirely in lowercase common words, so restricting
        // this to proper nouns and figures left that negation tying its own
        // positive at 1.0000.
        //
        // Echoed tokens are excluded: the question's own subject is not part of
        // the claim, and counting it halved the measured contradiction.
        if !ta.boiler[i]
            && !ta.echo[i]
            && ta.w[i] >= p.decisive_min
            && ta.supw[i] > 0.0
            && ta.supi[i] > 0
        {
            let k = (ta.supi[i] - 1) as usize;
            if k < tg.n {
                let w = ta.w[i] * ta.supw[i];
                sup_mass += w;
                // `supi` is whichever occurrence the hash set happened to
                // store, not the one the answer meant, so a ground truth that
                // uses a word both ways reported a correct answer as
                // contradicting itself. An abuse-history record uses several:
                // "do not have a public geographic location ... are used only
                // ... does not appear". Measured on CLEAN-PAIR fixture 01, a
                // faithful terse answer scored polarity 0.8558 and the fixture
                // 0.9436, breaking the equivalence tolerance.
                //
                // A contradiction is real only when the ground truth states this
                // token *somewhere* and nowhere with the answer's polarity. If
                // the token has no literal occurrence at all it was matched
                // through an alias — the acronym rule, the ISO table, a stem —
                // and there is no polarity to compare, so it abstains. Scanning
                // without that guard read `US` against `United States` as a
                // contradiction and cost a correct rewording 0.055.
                let (mut literal, mut agrees) = (false, false);
                let mut j = 0usize;
                while j < tg.n {
                    if tg.hash[j] == ta.hash[i] || tg.stem[j] == ta.stem[i] {
                        literal = true;
                        if tg.neg[j] == ta.neg[i] {
                            agrees = true;
                            break;
                        }
                    }
                    j += 1;
                }
                if literal && !agrees {
                    contra_mass += w;
                }
            }
        }
        i += 1;
    }
    let base = if sup_mass <= 0.0 {
        1.0
    } else {
        clamp01(1.0 - p.m_contra * (contra_mass / sup_mass))
    };
    if verdict_flip {
        return base * p.m_verdict_flip;
    }
    base
}

/// Weighted fraction of the answer's own content that the ground truth supports.
///
/// Question-echoed tokens are **not** discounted here. Measured over 554 real
/// rows, bag-of-words overlap with the question correlates *negatively* (-0.258)
/// with the champion's score, so a general echo penalty would buy nothing and
/// would wreck the Spearman agreement the gate requires on multi-miner intents.
/// The parrot effect is positional, and the mechanism that catches it is
/// `answeredness` below — where the echo flag *is* used, and only there.
///
/// Measured over two separate pools. **Decisive** tokens — figures,
/// identifiers, proper nouns — are what the answer is right or wrong *about*.
/// Ordinary prose is style, and enters only at `prose_w`, because a correct but
/// wordy answer must not be diluted below a terse wrong one merely for using
/// more words (ARCHITECTURE A3.4: facts dominant, lexical overlap a low-weight
/// tie-breaker).
fn precision_of(ta: &Toks, gt_uncovered: f32, p: &Profile) -> f32 {
    let (mut fact_n, mut fact_d) = (0.0f32, 0.0f32);
    let (mut prose_n, mut prose_d) = (0.0f32, 0.0f32);
    let mut i = 0usize;
    while i < ta.n {
        if ta.boiler[i] {
            i += 1;
            continue;
        }
        let w = ta.w[i];
        if ta.decisive[i] {
            // An unsupported non-numeric assertion abstains when the answer has
            // already covered every entity the ground truth names. It has then
            // displaced nothing, so it is *unverifiable extra detail*, not a
            // wrong claim — the same substitution-versus-addition rule the
            // entity channel applies (A3.8, precision not recall). A swapped
            // entity always leaves ground-truth mass uncovered, so it is still
            // charged here; it is only the answer that says everything the truth
            // says, and then says more, that goes free. This was the entire
            // residual gap on verbose-but-correct answers: "the IP address ..."
            // scored precision 0.9066 with every entity right, purely for the
            // token "IP", which the entity channel already abstains on.
            //
            // Figures are exempt: the numeric channel grades them by value and
            // has its own comparability rule, so leaving them in precision costs
            // a correct answer nothing and keeps a bare wrong figure visible.
            // A pure addition enters the denominator at `add_w` rather than in
            // full, so it dilutes precision a little instead of reading as a
            // wrong claim. At the previous full abstain it cost nothing at all.
            let w = if ta.supw[i] <= 0.0 && ta.kind[i] != K_NUMBER && gt_uncovered <= 0.0 {
                w * p.add_w
            } else {
                w
            };
            fact_d += w;
            fact_n += w * ta.supw[i];
        } else {
            prose_d += w;
            prose_n += w * ta.supw[i];
        }
        i += 1;
    }

    match (fact_d > 0.0, prose_d > 0.0) {
        (true, true) => {
            clamp01((1.0 - p.prose_w) * (fact_n / fact_d) + p.prose_w * (prose_n / prose_d))
        }
        // An answer of pure assertions, or of pure prose: score what it has.
        (true, false) => clamp01(fact_n / fact_d),
        (false, true) => clamp01(prose_n / prose_d),
        _ => 0.0,
    }
}

/// Does the answer contribute anything beyond the question and the boilerplate?
///
/// This is a *gate*, not a recall term: a little genuine novel supported content
/// opens it fully. It only stays shut when the answer adds nothing — the empty
/// answer, the content-filter refusal, and the contentless question-echo that
/// the live champion scores 0.9933 (ARCHITECTURE A3.6, the headline exhibit).
/// This is the **only** place the question-echo flag is consulted.
///
/// Crucially the gate is conditioned on the ground truth. In real traffic the
/// refusals are usually the *ground truths*, not the answers (8 of 15 weather
/// GTs are hedged, and 40 of 58 sub-0.02 rows). When the ground truth itself
/// carries no decisive content there is nothing to be found, a hedged answer is
/// the correct answer, and the gate opens fully — scoring falls back to text
/// agreement. We never relitigate the ground truth.
fn answeredness(ta: &Toks, tg: &Toks, sq: &Set, p: &Profile) -> f32 {
    // The ground truth's own answer-bearing mass: what it says that the question
    // did not already give away.
    // Novelty is counted at full weight for an assertion (a figure, identifier
    // or proper noun) and heavily discounted for ordinary prose. Without that
    // split, a parrot padded with generic filler earns real novelty credit
    // whenever the ground truth is long enough to contain the same common
    // words — measured on live traffic, a contentless echo reached 0.80 on this
    // gate purely through words like "terms", "scope" and "order".
    let mass = |t: &Toks, i: usize, prose_w: f32| -> f32 {
        if t.decisive[i] {
            t.w[i]
        } else {
            t.w[i] * prose_w
        }
    };

    // Does the ground truth state decisive facts of its own that the question
    // did not already give away?
    let (mut gt_ans, mut gt_decisive) = (0.0f32, 0.0f32);
    let mut k = 0usize;
    while k < tg.n {
        if tg.w[k] >= p.decisive_min && !sq.contains_tok(tg, k) {
            gt_ans += mass(tg, k, p.novel_prose_w);
            if tg.decisive[k] {
                gt_decisive += tg.w[k];
            }
        }
        k += 1;
    }

    // When it does, novelty is counted from decisive content ONLY. Prose
    // agreement is not assertion: a ground-truth-blind list of the intent's own
    // field names ("country, region, city, latitude, longitude, ISP ...")
    // carries no figure, identifier or proper noun, yet 81% of it appeared
    // somewhere in a long ground truth and it scored a perfect 1.0 on recorded
    // rows (adversarial review C5). If the truth states facts, an answer that
    // states none of them has not answered.
    let novel_prose_w = if gt_decisive >= p.gt_decisive_min {
        p.novel_prose_w_gt
    } else {
        p.novel_prose_w
    };
    if gt_ans < p.gt_decisive_min {
        // Refusal-shaped ground truth: no answer can be "unanswered" against it.
        return 1.0;
    }

    let mut novel = 0.0f32;
    let mut i = 0usize;
    while i < ta.n {
        if !ta.boiler[i] && !ta.echo[i] && ta.w[i] >= p.decisive_min {
            novel += mass(ta, i, novel_prose_w) * ta.supw[i];
        }
        i += 1;
    }

    let sat = fmax(fmin(p.ans_sat, gt_ans * p.ans_gt_frac), p.ans_sat_min);
    // A floor under the gate: a shut gate must not collapse every non-answer
    // onto the identical value, or the ordering below it is lost to ties.
    p.ans_floor + (1.0 - p.ans_floor) * smoothstep01(novel / sat)
}

#[cfg(test)]
mod tests {
    use super::*;

    const Q: &[u8] = b"Can you look up the geolocation details for the IP address 142.251.42.174 and provide the country, city, and ISP information?";
    const GT: &[u8] = b"The IP address 142.251.42.174 is associated with Google LLC and is located in the United States. The ISP is clearly identified as Google LLC.";

    #[test]
    fn self_match_is_maximal() {
        let s = score(Q, GT, GT);
        assert!(s >= 0.75, "self-match {} must clear the 0.75 ratchet", s);
    }

    #[test]
    fn self_match_beats_an_unrelated_answer() {
        let self_m = score(Q, GT, GT);
        let cross = score(
            Q,
            GT,
            b"The recipe calls for two cups of flour and a pinch of salt.",
        );
        assert!(self_m > cross, "{} must beat {}", self_m, cross);
    }

    #[test]
    fn a_correct_answer_beats_a_wrong_location() {
        let right = score(
            Q,
            GT,
            b"The data shows the IP 142.251.42.174 is hosted by Google LLC in the United States.",
        );
        let wrong = score(
            Q,
            GT,
            b"The data shows the IP 142.251.42.174 is hosted by Cloudflare in Mumbai, India.",
        );
        assert!(right > wrong, "right {} must beat wrong {}", right, wrong);
    }

    #[test]
    fn a_question_echo_scores_near_zero() {
        // The champion's known hole: this contentless restatement scores 0.9933
        // live. It asserts nothing the question did not already contain.
        let echo = score(
            Q,
            GT,
            b"The data shows the geolocation details for the IP address 142.251.42.174 including country, city and ISP information.",
        );
        let real = score(
            Q,
            GT,
            b"The data shows the IP is hosted by Google LLC in the United States.",
        );
        assert!(
            echo < 0.25,
            "question echo scored {}, must be near zero",
            echo
        );
        assert!(
            real > echo * 2.0,
            "a real answer {} must clear the echo {}",
            real,
            echo
        );
    }

    #[test]
    fn the_content_filter_refusal_scores_near_zero() {
        let s = score(
            Q,
            GT,
            b"- The generated text has been blocked by our content filters.",
        );
        assert!(s < 0.1, "content-filter refusal scored {}", s);
    }

    #[test]
    fn keyword_stuffing_loses_to_a_real_answer() {
        let stuffed = score(
            Q,
            GT,
            b"IP address geolocation country city ISP information lookup details network region location provider",
        );
        let real = score(
            Q,
            GT,
            b"The data shows the IP is hosted by Google LLC in the United States.",
        );
        assert!(
            real > stuffed,
            "real {} must beat stuffed {}",
            real,
            stuffed
        );
    }

    #[test]
    fn a_wrong_figure_scores_below_the_right_one() {
        let q = b"What is the CVSS score for CVE-2021-44228?";
        let gt = b"The CVSS score for CVE-2021-44228 is 10, indicating critical severity. Affected versions include Apache Log4j up to 2.14.1.";
        let right = score(q, gt, b"The data shows CVE-2021-44228 has a CVSS score of 10 and is critical in Apache Log4j.");
        let wrong = score(q, gt, b"The data shows CVE-2021-44228 has a CVSS score of 7.5 and is critical in Apache Log4j.");
        assert!(right > wrong, "right {} must beat wrong {}", right, wrong);
    }

    #[test]
    fn format_equivalence_holds_within_tolerance() {
        // Same facts, three registers. ARCHITECTURE A4: JSON and prose with equal
        // facts must score equally.
        let prose = score(
            Q,
            GT,
            b"The IP is hosted by Google LLC and located in the United States.",
        );
        let json = score(
            Q,
            GT,
            b"{\"isp\":\"Google LLC\",\"country\":\"United States\"}",
        );
        assert!(
            fabs(prose - json) < 0.35,
            "prose {} vs json {}",
            prose,
            json
        );
    }

    #[test]
    fn scores_spread_rather_than_collapsing() {
        // The gate rejects a flat scorer (stddev must exceed 0.05).
        let answers: [&[u8]; 5] = [
            GT,
            b"The data shows the IP is hosted by Google LLC in the United States.",
            b"The data shows the IP is hosted by Cloudflare in Mumbai, India.",
            b"- The generated text has been blocked by our content filters.",
            b"The recipe calls for two cups of flour.",
        ];
        let mut lo = 1.0f32;
        let mut hi = 0.0f32;
        for a in answers.iter() {
            let s = score(Q, GT, a);
            lo = fmin(lo, s);
            hi = fmax(hi, s);
        }
        assert!(hi - lo > 0.05, "spread was only {}", hi - lo);
    }

    #[test]
    fn results_do_not_depend_on_call_order() {
        let a = score(
            Q,
            GT,
            b"The data shows the IP is hosted by Google LLC in the United States.",
        );
        let _ = score(
            b"unrelated question",
            b"unrelated ground truth",
            b"unrelated answer",
        );
        let b = score(
            Q,
            GT,
            b"The data shows the IP is hosted by Google LLC in the United States.",
        );
        assert_eq!(a, b, "stale scratch state leaked between calls");
    }

    #[test]
    fn a_regrouped_figure_is_not_an_exact_match() {
        // The exact-match short-circuit used to fold punctuation, so each of
        // these returned a literal 1.0 for a wrong answer (review C1).
        let q = b"What is the CVSS score?";
        let gt = b"The CVSS score is 10.";
        let wrong = score(q, gt, b"The CVSS score is 1.0");
        let right = score(q, gt, b"The CVSS score is 10.");
        assert_eq!(right, 1.0);
        assert!(
            wrong < 0.9,
            "CVSS 1.0 against a truth of 10 scored {}",
            wrong
        );
    }

    #[test]
    fn a_negated_claim_does_not_tie_the_correct_one() {
        // One word flips the meaning; before the polarity term both scored
        // 1.0000 (review C2).
        let q = b"Where is the IP 8.8.8.8?";
        let gt = b"The IP 8.8.8.8 is located in Germany.";
        let pos = score(
            q,
            gt,
            b"The data shows the IP 8.8.8.8 is located in Germany.",
        );
        let neg = score(
            q,
            gt,
            b"The data shows the IP 8.8.8.8 is not located in Germany.",
        );
        assert!(pos > neg, "positive {} must beat negated {}", pos, neg);
        assert!(neg < 0.75, "a flat contradiction scored {}", neg);
    }

    #[test]
    fn a_ground_truth_blind_field_list_is_not_an_answer() {
        // A keyword blob written from the intent's field names, with no lookup
        // and no knowledge of any ground truth, scored 1.0 on live rows (C5).
        let q = b"Can you look up the geolocation for the IP address 91.146.179.123?";
        let gt = b"The IP address 91.146.179.123 resolves to Reykjavik, Capital Region, Iceland. It is announced by Ljosleidarinn ehf (AS22057).";
        let blob = score(q, gt,
            b"The data shows the country, region, city, latitude, longitude, coordinates, ISP, organisation, autonomous system network, hosting provider, timezone, postal code, continent and address associated with this IP address, including its allocation, registry, abuse contact and reported activity.");
        let real = score(q, gt, b"The data shows the IP resolves to Reykjavik, Capital Region, Iceland, announced by Ljosleidarinn ehf.");
        assert!(
            real > blob,
            "a real answer {} must beat the field-name blob {}",
            real,
            blob
        );
        assert!(blob < 0.5, "field-name blob scored {}", blob);
    }

    #[test]
    fn coordinates_without_a_degree_sign_still_count() {
        // The plain-text form was scored 0.0000, identical to the wrong
        // hemisphere (review M5).
        let q = b"What are the coordinates?";
        let gt = b"Approximate coordinates are -34.9011, -56.1645.";
        let plain = score(q, gt, b"The data shows coordinates 34.9011S, 56.1645W.");
        let wrong = score(q, gt, b"The data shows coordinates 34.9011N, 56.1645E.");
        assert!(
            plain > 0.5,
            "correct plain-text coordinates scored {}",
            plain
        );
        assert!(
            plain > wrong,
            "right {} must beat wrong hemisphere {}",
            plain,
            wrong
        );
    }

    const EQ: &[u8] = b"Where is 8.8.8.8 located?";
    const EGT: &[u8] = b"The data shows 8.8.8.8 is located in Mountain View, California, United States, operated by Google LLC.";

    #[test]
    fn a_single_swapped_entity_is_a_contradiction_not_a_rounding_error() {
        // Pre-flight repro: each of these scored a literal 1.0000, tying the
        // verbatim-correct answer, because a proper noun was only unsupported
        // precision mass and never reached a multiplicative channel.
        let right = score(EQ, EGT, EGT);
        let city = score(EQ, EGT, b"The data shows 8.8.8.8 is located in Berlin, California, United States, operated by Google LLC.");
        let isp = score(EQ, EGT, b"The data shows 8.8.8.8 is located in Mountain View, California, United States, operated by Cloudflare Inc.");
        let country = score(EQ, EGT, b"The data shows 8.8.8.8 is located in Mountain View, California, Germany, operated by Google LLC.");
        assert_eq!(right, 1.0);
        for (label, s) in [("city", city), ("isp", isp), ("country", country)] {
            assert!(
                s < 0.5,
                "single {} swap scored {}, must be far below correct",
                label,
                s
            );
        }
    }

    #[test]
    fn a_correctly_reworded_answer_outranks_every_wrong_variant() {
        // "US" for "United States" and "run by" for "operated by" are legitimate
        // variation, not error. Before the acronym rule this scored 0.1956 --
        // below the wrong answers.
        let reworded = score(
            EQ,
            EGT,
            b"The data shows 8.8.8.8 resolves to Mountain View, California, US, run by Google LLC.",
        );
        let worst_wrong = score(EQ, EGT, b"The data shows 8.8.8.8 is located in Berlin, California, United States, operated by Google LLC.");
        // The ordering must hold under every profile.
        assert!(
            reworded > worst_wrong,
            "reworded {} vs wrong {}",
            reworded,
            worst_wrong
        );
        // The *ceiling* is profile-dependent, and deliberately so. STORM_ALERT
        // runs prose_w = 0.7 to keep its Spearman agreement with the incumbent
        // (tune.md), which is exactly the behaviour that costs a correct
        // rewording: measured 0.3812 here against 0.3404 for the wrong city --
        // ordered correctly, but nothing like the separation IP_GEOLOCATION
        // gets. That is the STORM trade recorded in tune.md, not a defect of
        // this fixture, so the strong bound is asserted where it is claimed.
        #[cfg(not(feature = "storm-alert"))]
        {
            assert!(reworded > 0.75, "a correct rewording scored {}", reworded);
            assert!(
                reworded > worst_wrong * 2.0,
                "reworded {} vs wrong {}",
                reworded,
                worst_wrong
            );
        }
    }

    #[test]
    fn extra_unrequested_entities_stay_neutral() {
        // The guard on the entity channel: an answer that covers everything the
        // ground truth names and adds more has contradicted nothing (A3.8).
        let extra = score(EQ, EGT, b"The data shows 8.8.8.8 is located in Mountain View, California, United States, operated by Google LLC, in the Santa Clara County area.");
        assert!(
            extra > 0.75,
            "an answer with extra true detail scored {}",
            extra
        );
    }

    #[test]
    fn more_wrong_entities_score_strictly_lower() {
        let one = score(EQ, EGT, b"The data shows 8.8.8.8 is located in Berlin, California, United States, operated by Google LLC.");
        let three = score(EQ, EGT, b"The data shows 8.8.8.8 is located in Berlin, Brandenburg, Germany, operated by Google LLC.");
        let all = score(EQ, EGT, b"The data shows 8.8.8.8 is located in Berlin, Brandenburg, Germany, operated by Deutsche Telekom.");
        assert!(one > three && three > all, "{} {} {}", one, three, all);
    }

    // ---- the clean-pair round (rejection of registration 1377) -------------
    //
    // The node's benchmark is clean good-vs-bad pairs, not the adversarial
    // corpus this suite grew from. On those, a correct answer must score ~1.0
    // however it is phrased. Each test below pins one defect that only a
    // *reworded* correct answer could ever hit: the verbatim copy short-circuits
    // on exact match and never reaches the code, which is why none of them
    // showed up until the class existed.

    #[cfg(not(feature = "storm-alert"))]
    #[test]
    fn a_correct_answer_is_not_charged_for_prose_the_truth_omits() {
        // Measured before: verbatim 1.0000, the same facts reworded 0.8785.
        for phrasing in [
            &b"The data shows 8.8.8.8 resolves to Mountain View, California, US, run by Google LLC."[..],
            &b"Mountain View, California, United States. Google LLC."[..],
            &b"{\"city\":\"Mountain View\",\"region\":\"California\",\"country\":\"United States\",\"isp\":\"Google LLC\"}"[..],
            &b"According to the geolocation record, the IP address 8.8.8.8 is situated in Mountain View, in the state of California, within the United States, and the network is operated by Google LLC, a well known provider."[..],
        ] {
            let s = score(EQ, EGT, phrasing);
            assert!(s > 0.99, "a correct rephrasing scored {}", s);
        }
    }

    #[cfg(not(feature = "storm-alert"))]
    #[test]
    fn one_extra_true_identifier_costs_far_less_than_a_swapped_one() {
        // The identifier channel had no substitution rule, so an answer that
        // quoted the truth's IP *and* added the AS number scored 0.4876 — the
        // same column as a wrong fact. It is now an *addition*, which costs
        // `add_w` rather than reading as a substitution.
        //
        // It is deliberately not free. At add_w = 0 an answer could append an
        // invented IP or ASN for nothing (measured 0.9999), and nothing in the
        // text distinguishes an invented identifier from a true one the ground
        // truth omits. So the price is paid by both, and the test pins the
        // *ordering* rather than a ceiling: adding true detail must cost far
        // less than getting the identifier wrong.
        let extra = score(EQ, EGT, b"The data shows 8.8.8.8 is located in Mountain View, California, United States, operated by Google LLC (AS15169).");
        let swapped = score(EQ, EGT, b"The data shows 1.1.1.1 is located in Mountain View, California, United States, operated by Google LLC.");
        let clean = score(
            EQ,
            EGT,
            b"The data shows 8.8.8.8 resolves to Mountain View, California, US, run by Google LLC.",
        );
        assert!(extra > 0.75, "extra true identifier scored {}", extra);
        assert!(swapped < 0.5, "a swapped IP scored {}", swapped);
        assert!(
            extra > swapped * 1.5,
            "extra {} vs swapped {}",
            extra,
            swapped
        );
        assert!(
            clean > extra,
            "an answer that adds nothing {} should lead {}",
            clean,
            extra
        );
    }

    #[cfg(not(feature = "storm-alert"))]
    #[test]
    fn a_wrong_organisation_acronym_is_still_a_wrong_entity() {
        // ALL-CAPS tokens used to abstain wholesale, so "AWS" for "Google LLC"
        // scored 0.9829 while "Cloudflare Inc." scored 0.2248 for the same swap.
        let acronym = score(EQ, EGT, b"The data shows 8.8.8.8 is located in Mountain View, California, United States, operated by AWS.");
        let spelled = score(EQ, EGT, b"The data shows 8.8.8.8 is located in Mountain View, California, United States, operated by Cloudflare Inc.");
        assert!(
            acronym < 0.5,
            "a wrong ISP written as an acronym scored {}",
            acronym
        );
        assert!(
            fabs(acronym - spelled) < 0.25,
            "the same swap scored {} vs {}",
            acronym,
            spelled
        );
        // A two-letter geographic code is still exempt: no lexical rule reaches
        // "UY" from "Uruguay", so it must abstain rather than read as a swap.
        let q = b"Where does 13.36.15.128 resolve to?";
        let gt = b"The IP address 13.36.15.128 resolves to Montevideo, Montevideo, Uruguay, announced by Administracion Nacional.";
        let code = score(q, gt, b"The data shows 13.36.15.128 is located in Montevideo in Montevideo, UY, and is run by Administracion Nacional.");
        let swap = score(q, gt, b"The data shows 13.36.15.128 resolves to Osaka, Montevideo, Uruguay, announced by Administracion Nacional.");
        assert!(
            code > swap * 2.0,
            "country code {} vs swapped city {}",
            code,
            swap
        );
    }

    #[cfg(not(feature = "storm-alert"))]
    #[test]
    fn a_range_is_only_a_hedge_if_it_is_wider_than_the_truth() {
        // A hyphenated postal code parses as a range, and the hedge discount was
        // charged on absolute width — so quoting the truth's own "162-0843" back
        // scored the numeric channel 0.382 and the answer 0.4632.
        let q = b"Where is 142.251.42.174?";
        let gt = b"The IP 142.251.42.174 is located in the Shimo area (postal code 162-0843) and is run by Google LLC.";
        let s = score(q, gt, b"The data shows 142.251.42.174 sits in the Shimo area, postal code 162-0843, operated by Google LLC.");
        assert!(
            s > 0.95,
            "restating the truth's own hyphenated figure scored {}",
            s
        );
        // A range genuinely wider than the truth is still a hedge.
        let wq = b"What is the sustained wind speed?";
        let wgt = b"Sustained wind reaches 47 km/h.";
        let tight = score(wq, wgt, b"Sustained wind reaches 46-48 km/h.");
        let hedge = score(wq, wgt, b"Sustained wind reaches 5-90 km/h.");
        assert!(
            tight > hedge * 2.0,
            "tight range {} vs hedge {}",
            tight,
            hedge
        );
    }

    #[cfg(not(feature = "storm-alert"))]
    #[test]
    fn a_list_marker_is_not_a_figure_with_an_unknown_unit() {
        // "2. Using tools" read as the quantity 2 in a made-up category, so an
        // answer writing "2. tools" disagreed on a dimension neither side has
        // and the numeric channel fell to 0.145.
        let q = b"What should I do about this IP?";
        let gt = b"Consider: 1. Checking timestamps for suspicious activity. 2. Using tools like VirusTotal to scan associated URLs. 3. Reporting to AbuseIPDB.";
        let s = score(q, gt, b"Consider: 1. Checking timestamps for suspicious activity. 2. tools like VirusTotal to scan associated URLs. 3. Reporting to AbuseIPDB.");
        assert!(s > 0.9, "a list-marked answer scored {}", s);
        // A unit that actually abuts its figure is still read as one.
        let wq = b"What is the sustained wind speed?";
        let wgt = b"Sustained wind reaches 47 km/h.";
        let fake = score(wq, wgt, b"Sustained wind reaches 47 bananas.");
        let honest = score(wq, wgt, b"Sustained wind reaches 47 m/s.");
        assert!(fake < 0.01, "a fake unit scored {}", fake);
        assert!(
            fake <= honest * 4.0,
            "fake {} must not beat honest-wrong {}",
            fake,
            honest
        );
    }

    // ---- the audit round: notation must not decide correctness -------------

    #[cfg(not(feature = "storm-alert"))]
    #[test]
    fn typography_does_not_decide_correctness() {
        // Bytes >= 0x80 are opaque word bytes, so a curly apostrophe glued its
        // neighbours into one token where the ASCII one split them: the same
        // answer scored 0.9997 curly and 0.2592 with `'`.
        let q = b"Where is 142.251.42.174?";
        let gt = "The IP 142.251.42.174 is located in the Shimo\u{2019}ochiai area of Tokyo, Japan, operated by Google LLC.".as_bytes();
        let curly = score(q, gt, "The data shows 142.251.42.174 sits in the Shimo\u{2019}ochiai area of Tokyo, Japan, run by Google LLC.".as_bytes());
        let ascii = score(q, gt, b"The data shows 142.251.42.174 sits in the Shimo'ochiai area of Tokyo, Japan, run by Google LLC.");
        assert!(ascii > 0.99, "the ASCII apostrophe form scored {}", ascii);
        assert!(
            fabs(curly - ascii) < 0.01,
            "curly {} vs ascii {}",
            curly,
            ascii
        );
    }

    #[cfg(not(feature = "storm-alert"))]
    #[test]
    fn hemisphere_notation_equals_a_signed_coordinate() {
        // `s`, `n` and `e` all name units, so the unit table claimed them and
        // only `W` ever read as a hemisphere: a correct southern latitude scored
        // 0.2055 against a signed ground truth.
        let q = b"What are the coordinates?";
        let gt = b"Approximate coordinates are -34.9011, -56.1645 in Montevideo, Uruguay.";
        for form in [
            &b"The data shows coordinates 34.9011 S, 56.1645 W in Montevideo, Uruguay."[..],
            &b"The data shows coordinates 34.9011S, 56.1645W in Montevideo, Uruguay."[..],
            &b"The data shows latitude -34.9011 and longitude -56.1645 in Montevideo, Uruguay."[..],
        ] {
            let s = score(q, gt, form);
            assert!(s > 0.99, "an equivalent coordinate form scored {}", s);
        }
        let wrong = score(
            q,
            gt,
            b"The data shows coordinates 34.9011 N, 56.1645 E in Montevideo, Uruguay.",
        );
        assert!(wrong < 0.25, "the wrong hemisphere scored {}", wrong);
        // The guard that keeps a duration a duration.
        let dq = b"How long is the window?";
        let dgt = b"The window is 30 s.";
        assert!(score(dq, dgt, b"The window is 30 seconds.") > 0.9);
        assert!(score(dq, dgt, b"The window is 45 s.") < 0.1);
    }

    #[cfg(not(feature = "storm-alert"))]
    #[test]
    fn a_country_code_is_checked_not_ignored() {
        let q = b"Where does 13.36.15.128 resolve to?";
        let gt = b"The IP address 13.36.15.128 resolves to Montevideo, Montevideo, Uruguay, announced by Administracion Nacional.";
        let right = score(q, gt, b"The data shows 13.36.15.128 is located in Montevideo in Montevideo, UY, and is run by Administracion Nacional.");
        let wrong = score(q, gt, b"The data shows 13.36.15.128 is located in Montevideo in Montevideo, DE, and is run by Administracion Nacional.");
        assert!(right > 0.99, "the correct ISO code scored {}", right);
        assert!(wrong < 0.5, "a wrong ISO code scored {}", wrong);
        // Method vocabulary is not a country: two-letter tokens we cannot check
        // must still abstain rather than read as wrong entities.
        let method = score(q, gt, b"The data shows the IP address 13.36.15.128 resolves to Montevideo, Montevideo, Uruguay, announced by Administracion Nacional per the RIR WHOIS record.");
        assert!(method > 0.95, "method vocabulary cost {}", method);
        // And the collision that made this dangerous: `is` is Iceland's code.
        // Reading codes without a case test put "iceland" in every ground truth
        // containing the verb, and a swapped country scored a perfect 1.0.
        let swapped = score(q, gt, b"The data shows the IP address 13.36.15.128 resolves to Montevideo, Montevideo, Iceland, announced by Administracion Nacional.");
        assert!(swapped < 0.5, "a swapped country scored {}", swapped);
    }

    // The generic profile keeps `ss_hi` at 0.92, so both sides saturate to
    // exactly 1.0 and the comparison cannot be made there. Asserted on the
    // registration target, whose smoothstep runs the full range.
    #[cfg(feature = "ip-geolocation")]
    #[test]
    fn an_appended_false_fact_is_not_free() {
        let base = &b"The data shows 8.8.8.8 is located in Mountain View, California, United States, operated by Google LLC"[..];
        let clean = score(EQ, EGT, b"The data shows 8.8.8.8 is located in Mountain View, California, United States, operated by Google LLC.");
        for (label, tail) in [
            ("extra IP", &b", alongside 203.0.113.9."[..]),
            ("extra ASN", &b" (AS64512)."[..]),
            ("extra country", &b", within Germany."[..]),
            ("extra city", &b", in the Pyongyang area."[..]),
        ] {
            let mut buf = [0u8; 256];
            let n = base.len();
            buf[..n].copy_from_slice(base);
            buf[n..n + tail.len()].copy_from_slice(tail);
            let s = score(EQ, EGT, &buf[..n + tail.len()]);
            assert!(s < clean, "appending an {label} was free: {s} vs {clean}");
        }
    }

    #[test]
    fn an_empty_ground_truth_abstains() {
        assert_eq!(score(Q, b"", b"anything at all"), 0.0);
    }
}

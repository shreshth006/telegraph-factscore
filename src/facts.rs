//! Fact agreement: how far the figures and identifiers an answer *asserts* are
//! borne out by the ground truth.
//!
//! Agreement is graded, never a step: a near-miss stays high, a gross miss falls
//! away. A figure the ground truth has no comparable counterpart for is
//! **neutral**, not wrong - we score precision of what the answer asserts, not
//! recall of the truth (ARCHITECTURE A3.8).

#![allow(dead_code)]

use crate::bytes::*;
use crate::profile::Profile;
use crate::sets::Set;
use crate::tokens::{Toks, K_IDENT, K_NUMBER};
use crate::units::*;

// --------------------------------------------------------------------------
// Agreement
// --------------------------------------------------------------------------

/// One side of a comparison: a figure, possibly a range, with its unit and the
/// hash of any unit-shaped word we could not identify.
#[derive(Clone, Copy)]
pub struct Fig {
    pub lo: f32,
    pub hi: f32,
    pub unit: u8,
    /// Non-zero when a word sat where a unit would but named no unit we know.
    pub foreign: u32,
}

impl Fig {
    pub fn of(t: &Toks, i: usize) -> Fig {
        Fig {
            lo: t.val[i],
            hi: t.vhi[i],
            unit: t.unit[i],
            foreign: t.ufword[i],
        }
    }
}

/// Graded agreement between an asserted figure and a candidate one, or `None`
/// when the two are not talking about the same quantity at all.
///
/// Comparability is decided *before* magnitude, which is the whole point: two
/// wind speeds are comparable however far apart they are, so 47 m/s against a
/// ground truth of 5 m/s is a wrong answer rather than an unverifiable one.
/// Only when neither side carries a unit do we fall back on a magnitude band to
/// guess whether the figures are about the same thing.
pub fn value_agreement(a: Fig, g: Fig, p: &Profile) -> Option<f32> {
    let (ad, gd) = (dimension(a.unit), dimension(g.unit));
    if ad != D_NONE && gd != D_NONE && ad != gd {
        // A temperature is not a near-miss for a wind speed; it is unrelated.
        return None;
    }

    // Agreement between two intervals. A plain figure is the degenerate case
    // lo == hi, so a range containing the truth agrees fully and one that misses
    // decays from whichever bound is nearer (adversarial review M6).
    let rate = |alo: f32, ahi: f32, glo: f32, ghi: f32| -> (f32, f32) {
        let gap = if ahi < glo {
            glo - ahi
        } else if alo > ghi {
            alo - ghi
        } else {
            0.0
        };
        let scale = fmax(fmax(fabs(glo), fabs(ghi)), 1e-6);
        let rel = gap / scale;
        let mut score = if gap <= p.num_abs_tol || rel <= p.num_rel_tol {
            1.0
        } else {
            // 1/(1 + k*rel): smooth, bounded, and needs no transcendental.
            1.0 / (1.0 + p.num_rel_k * rel)
        };
        // A range earns credit for containing the truth, but a range wide enough
        // to contain almost anything is a hedge, not an answer. Discount by the
        // width the answer adds OVER the ground truth's own, so "5-50 m/s"
        // cannot bank the same credit as "46-48 m/s".
        //
        // The excess is what matters, not the absolute width. Charging absolute
        // width punished an answer for restating a hyphenated figure the truth
        // itself states: a Japanese postal code "162-0843" parses as the range
        // 162..843, and quoting it back verbatim scored 0.382 on the numeric
        // channel — dragging an otherwise perfect answer to 0.4632 (measured,
        // CLEAN-PAIR fixture ip_geolocation-cleanpair-10). Verbatim copies were
        // spared only because the exact-match short-circuit never reaches here,
        // so every *reworded* correct answer paid it and no test saw it.
        let width = fmax((ahi - alo) - (ghi - glo), 0.0) / scale;
        if width > 0.0 {
            score /= 1.0 + p.m_range_width * width;
        }
        (score, rel)
    };

    let conv = |v: f32, u: u8| canonical(v, u);

    if ad != D_NONE && gd != D_NONE {
        // Same dimension: convert and compare. Always comparable, however far
        // apart — 47 m/s against 5 m/s is a wrong speed, not an unknown one.
        return Some(
            rate(
                conv(a.lo, a.unit),
                conv(a.hi, a.unit),
                conv(g.lo, g.unit),
                conv(g.hi, g.unit),
            )
            .0,
        );
    }

    if (ad != D_NONE) != (gd != D_NONE) {
        // Exactly one side named a unit. A bare figure may already be stated in
        // the canonical unit (`wind_kmh=128.7` beside `128.7 km/h`) or may need
        // converting, so both readings are tried — except for a percentage,
        // where the `%` is explicit and unambiguous. Allowing the raw reading
        // there let `42%` match both `0.42` and `42`, covering two scales at
        // once for free (adversarial review M4).
        let dim = if ad != D_NONE { ad } else { gd };
        let converted = rate(
            conv(a.lo, a.unit),
            conv(a.hi, a.unit),
            conv(g.lo, g.unit),
            conv(g.hi, g.unit),
        )
        .0;
        let best = if dim == D_FRAC {
            converted
        } else {
            fmax(rate(a.lo, a.hi, g.lo, g.hi).0, converted)
        };

        // The discount is asymmetric: it applies only when the **answer** is the
        // side that failed to name a unit. An answer that says `42%` against a
        // bare ground truth of `0.42` was explicit and is simply right.
        //
        // A figure whose unit we could not identify is asserting a quantity in
        // some other category, and a bare figure is weaker evidence than a
        // properly-united one. Neither may outrank an honest wrong value: before
        // this, `47 bananas` scored ~0.97 against a ground truth of `47 km/h`
        // while an honest `47 m/s` scored 0.015 — a 65x premium for a category
        // error (adversarial review C6).
        let discount = if ad != D_NONE {
            1.0
        } else if a.foreign != 0 && a.foreign != g.foreign {
            p.m_foreign_unit
        } else {
            p.m_bare_unit
        };
        return Some(best * discount);
    }

    // Neither side named a unit. If both carry the same unrecognised unit word
    // they are at least talking about the same thing.
    let (score, rel) = rate(a.lo, a.hi, g.lo, g.hi);
    if rel > p.num_band_rel {
        // Unitless and orders of magnitude apart: almost certainly a different
        // quantity (a year beside a CVSS score), so the answer is unverifiable
        // here rather than wrong.
        return None;
    }
    if a.foreign != 0 && g.foreign != 0 && a.foreign != g.foreign {
        return Some(score * p.m_foreign_unit);
    }
    Some(score)
}

/// Best agreement this answer figure achieves against any comparable
/// ground-truth figure. `None` when the ground truth offers nothing comparable.
///
/// When the answer's figure names a dimension and the ground truth states any
/// figure in that same dimension, only those are considered. Without this, a
/// wrong "over the next **48** hours" quietly matches a ground-truth latitude of
/// **47.8864** — a right figure attached to entirely the wrong entity.
pub fn best_agreement(ta: &Toks, i: usize, tg: &Toks, p: &Profile) -> Option<f32> {
    let ad = dimension(ta.unit[i]);
    let mut restrict = false;
    if ad != D_NONE {
        let mut k = 0usize;
        while k < tg.n {
            if tg.kind[k] == K_NUMBER && dimension(tg.unit[k]) == ad {
                restrict = true;
                break;
            }
            k += 1;
        }
    }

    // A percentage is the one dimension whose figures are routinely written
    // without their unit: a ground truth states `0.93` where the answer says
    // `93 percent`, and the dimension restriction then compared that 93% against
    // the *other* percentage in the truth (`7%`), scoring a correct answer 0.018.
    // A bare figure asserts no dimension, and `value_agreement` already holds the
    // percentage to its converted reading alone, so `93 percent` matches a bare
    // 0.93 and still does not match a bare 93.
    let bare_ok = ad == D_FRAC;

    let mut best: Option<f32> = None;
    let mut k = 0usize;
    while k < tg.n {
        let comparable = !restrict
            || dimension(tg.unit[k]) == ad
            || (bare_ok && dimension(tg.unit[k]) == D_NONE);
        if tg.kind[k] == K_NUMBER && comparable {
            if let Some(a) = value_agreement(Fig::of(ta, i), Fig::of(tg, k), p) {
                best = Some(match best {
                    Some(b) if b >= a => b,
                    _ => a,
                });
            }
        }
        k += 1;
    }
    best
}

/// The multiplicative fact term. Numbers are graded; identifiers are exact.
///
/// Returns `(multiplier, raw_agreement)` — the second value is exposed only
/// through `breakdown_answer` for debugging.
pub fn fact_multiplier(ta: &Toks, tg: &Toks, sa: &Set, p: &Profile) -> (f32, f32) {
    let (mut num_w, mut num_a) = (0.0f32, 0.0f32);
    let (mut id_sup, mut id_uns) = (0.0f32, 0.0f32);
    let mut num_min = 1.0f32;

    // Identifier mass the ground truth states that the answer never mentions.
    // The identifier channel needs the same substitution-versus-addition rule
    // the entity channel has: an answer that quotes the IP the truth names and
    // *also* gives the AS number has invented nothing to contradict, while one
    // that quotes a different IP has. Before this, `tg.has_ident` alone put every
    // unmatched identifier in the wrong column, so a correct answer carrying one
    // extra true identifier scored 0.46 against 1.00 for the same answer without
    // it (A3.8: precision of the assertion, not recall of the truth).
    let mut gt_id_uncovered = 0.0f32;
    let mut k = 0usize;
    while k < tg.n {
        if tg.kind[k] == K_IDENT && !sa.contains_tok(tg, k) {
            gt_id_uncovered += tg.w[k];
        }
        k += 1;
    }

    let mut i = 0usize;
    while i < ta.n {
        if ta.boiler[i] {
            i += 1;
            continue;
        }
        match ta.kind[i] {
            K_NUMBER => {
                // `None` means the ground truth says nothing comparable, so the
                // figure is unverifiable rather than wrong and stays neutral.
                if let Some(best) = best_agreement(ta, i, tg, p) {
                    num_w += ta.w[i];
                    num_a += ta.w[i] * best;
                    num_min = fmin(num_min, best);
                }
            }
            // Identifiers admit no tolerance. They only enter the channel
            // when the ground truth states identifiers to be checked against.
            K_IDENT if tg.has_ident => {
                if ta.supw[i] > 0.0 {
                    id_sup += ta.w[i];
                } else {
                    id_uns += ta.w[i];
                }
            }
            _ => {}
        }
        i += 1;
    }

    // Channels combine **multiplicatively**, not by averaging: quoting the right
    // CVE id must not rescue a wrong CVSS score. Each channel's weight is how
    // far it is allowed to pull the result down, so a weight of 1.0 lets a
    // wholly-wrong channel zero the term and 0.0 disables it.
    let mut f = 1.0f32;
    if num_w > 0.0 {
        // Worst-case-leaning, not a plain mean. An answer that gets four figures
        // right and one decisive figure wrong is a wrong answer; averaging lets
        // the wrong one hide behind the others, which is exactly the FACT-SWAP
        // failure the whole design exists to catch (ARCHITECTURE A3.5).
        let mean = num_a / num_w;
        let agree = (1.0 - p.num_min_bias) * mean + p.num_min_bias * num_min;
        f *= clamp01(1.0 - p.num_channel_w * (1.0 - agree));
    }
    // Only the paired part of the unsupported mass is a substitution. The excess
    // is a pure addition: it displaced nothing, but at `add_w = 0` an answer
    // could append an invented IP or ASN to an otherwise perfect answer for
    // free (measured >= 0.9999), so it enters the denominator at a reduced
    // weight rather than not at all.
    let id_sub = fmin(id_uns, gt_id_uncovered);
    let id_add = fmax(id_uns - id_sub, 0.0);
    let id_w = id_sup + id_sub + p.add_w * id_add;
    if id_w > 0.0 {
        // Identifiers admit no tolerance, so the channel leans on its worst one
        // exactly as the numeric and entity channels do. Without this a correct
        // identifier averaged a wrong one away: quoting the right IP alongside a
        // wrong AS number scored the channel 0.5, and the answer 0.5347 — above
        // several genuinely correct answers.
        let id_mean = id_sup / id_w;
        let id_worst = if id_sub > 0.0 { 0.0 } else { 1.0 };
        let id_agree = (1.0 - p.id_min_bias) * id_mean + p.id_min_bias * id_worst;
        f *= clamp01(1.0 - p.id_channel_w * (1.0 - id_agree));
    }
    if num_w <= 0.0 && id_w <= 0.0 {
        // No typed facts on either side: the fact channel abstains entirely
        // rather than dragging a prose answer down.
        return (1.0, 1.0);
    }
    let f = clamp01(f);
    (p.fact_floor + (1.0 - p.fact_floor) * f, f)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::base;
    use crate::tokens::tokenize;

    fn f(v: f32, u: u8) -> Fig {
        Fig {
            lo: v,
            hi: v,
            unit: u,
            foreign: 0,
        }
    }
    fn rng(lo: f32, hi: f32, u: u8) -> Fig {
        Fig {
            lo,
            hi,
            unit: u,
            foreign: 0,
        }
    }
    fn foreign(v: f32, word: &str) -> Fig {
        Fig {
            lo: v,
            hi: v,
            unit: U_NONE,
            foreign: hash_str(word),
        }
    }

    #[test]
    fn parses_decimals_and_thousands() {
        assert_eq!(leading_number(b"10").0, 10.0);
        assert_eq!(leading_number(b"0.429").0, 0.429);
        assert_eq!(leading_number(b"1,000").0, 1000.0);
        // A version string is not a decimal: stop at the integer part.
        assert_eq!(leading_number(b"2.14.1").0, 2.0);
    }

    #[test]
    fn units_normalise_across_dimensions() {
        let p = base();
        assert!(value_agreement(f(18.0, U_KMH), f(5.0, U_MS), &p).unwrap() > 0.99);
        // A temperature is not a near-miss for a wind speed: not comparable.
        assert_eq!(value_agreement(f(23.0, U_TEMP_C), f(23.0, U_MS), &p), None);
    }

    #[test]
    fn same_dimension_figures_stay_comparable_however_far_apart() {
        let p = base();
        let a = value_agreement(f(47.0, U_MS), f(5.0, U_MS), &p);
        assert!(a.is_some(), "same-dimension figures are always comparable");
        assert!(a.unwrap() < 0.05, "and a gross miss must score near zero");
    }

    #[test]
    fn unitless_figures_far_apart_are_unverifiable_not_wrong() {
        let p = base();
        assert_eq!(
            value_agreement(f(2009.0, U_NONE), f(10.0, U_NONE), &p),
            None
        );
    }

    #[test]
    fn agreement_degrades_smoothly_not_in_a_cliff() {
        let p = base();
        let exact = value_agreement(f(10.0, U_NONE), f(10.0, U_NONE), &p).unwrap();
        let near = value_agreement(f(9.9, U_NONE), f(10.0, U_NONE), &p).unwrap();
        let off = value_agreement(f(7.5, U_NONE), f(10.0, U_NONE), &p).unwrap();
        let gross = value_agreement(f(95.0, U_NONE), f(10.0, U_NONE), &p).unwrap();
        assert_eq!(exact, 1.0);
        assert!(near > off && off > gross);
        assert!(gross < 0.05);
    }

    #[test]
    fn a_percentage_no_longer_matches_at_two_scales() {
        // 42% is 0.42, never 42. Taking the better of both readings let one
        // answer cover both scales for free (adversarial review M4).
        let p = base();
        assert!(value_agreement(f(42.0, U_PCT), f(0.42, U_NONE), &p).unwrap() > 0.99);
        assert!(value_agreement(f(42.0, U_PCT), f(42.0, U_NONE), &p).unwrap() < 0.2);
    }

    #[test]
    fn a_near_miss_is_no_longer_inside_tolerance() {
        // 9.8 and 10 are different answers for a scored severity (review M3).
        let p = base();
        assert!(value_agreement(f(9.8, U_NONE), f(10.0, U_NONE), &p).unwrap() < 0.95);
        assert!(value_agreement(f(9.99, U_NONE), f(10.0, U_NONE), &p).unwrap() > 0.99);
    }

    #[test]
    fn a_range_is_scored_as_a_range_not_as_its_floor() {
        // "5-50 m/s" contains the truth and must beat a flat wrong "5 m/s"
        // (adversarial review M6).
        let p = base();
        let wide = value_agreement(rng(5.0, 50.0, U_MS), f(47.0, U_MS), &p).unwrap();
        let tight = value_agreement(rng(46.0, 48.0, U_MS), f(47.0, U_MS), &p).unwrap();
        let floor_only = value_agreement(f(5.0, U_MS), f(47.0, U_MS), &p).unwrap();
        // The upper bound is visible: a range containing the truth beats a flat
        // wrong answer at its floor.
        assert!(
            wide > floor_only,
            "wide {} vs floor-only {}",
            wide,
            floor_only
        );
        // But a range wide enough to contain any outcome is a hedge, and must
        // not bank the same credit as one that actually pins the value down.
        assert!(tight > wide * 2.0, "tight {} vs wide hedge {}", tight, wide);
        assert!(
            tight > 0.9,
            "a tight range containing the truth is right: {}",
            tight
        );
        // A range that misses still decays from the nearer bound.
        let misses = value_agreement(rng(5.0, 10.0, U_MS), f(47.0, U_MS), &p).unwrap();
        assert!(misses < 0.5);
    }

    #[test]
    fn an_unknown_unit_never_beats_an_honest_wrong_value() {
        // "47 bananas" against a ground truth of "47 km/h" is a category error,
        // and must not outrank an honest wrong speed (adversarial review C6).
        let p = base();
        let honest_wrong = value_agreement(f(47.0, U_MS), f(47.0, U_KMH), &p).unwrap();
        let category_error = value_agreement(foreign(47.0, "bananas"), f(47.0, U_KMH), &p).unwrap();
        assert!(
            category_error <= honest_wrong * 1.5,
            "category error {} must not tower over honest wrong {}",
            category_error,
            honest_wrong
        );
        assert!(
            category_error < 0.1,
            "category error {} must be near zero",
            category_error
        );
        // A real pressure unit is a different dimension entirely: not comparable.
        assert_eq!(value_agreement(f(47.0, U_HPA), f(47.0, U_KMH), &p), None);
        assert!(value_agreement(f(23.1, U_TEMP_K), f(23.1, U_TEMP_C), &p).is_some());
    }

    #[test]
    fn a_bare_figure_is_weaker_evidence_than_a_united_one() {
        let p = base();
        let united = value_agreement(f(128.7, U_KMH), f(128.7, U_KMH), &p).unwrap();
        let bare = value_agreement(f(128.7, U_NONE), f(128.7, U_KMH), &p).unwrap();
        assert_eq!(united, 1.0);
        assert!(bare < united && bare > 0.5, "bare = {}", bare);
    }

    #[test]
    fn a_wrong_figure_multiplies_down_but_not_to_a_cliff() {
        let p = base();
        let mut tg = Toks::new();
        tokenize(b"The CVSS score is 10.", &mut tg);
        annotate_units(&mut tg);

        let mut right = Toks::new();
        tokenize(b"a CVSS score of 10", &mut right);
        annotate_units(&mut right);
        let mut wrong = Toks::new();
        tokenize(b"a CVSS score of 7.5", &mut wrong);
        annotate_units(&mut wrong);

        let mut sr = crate::sets::EMPTY_SET;
        sr.fill(&right);
        let (mr, _) = fact_multiplier(&right, &tg, &sr, &p);
        let mut sw = crate::sets::EMPTY_SET;
        sw.fill(&wrong);
        let (mw, _) = fact_multiplier(&wrong, &tg, &sw, &p);
        assert!(mr > mw, "right {} must beat wrong {}", mr, mw);
        assert!(mw > p.fact_floor * 0.9, "wrong must not fall off a cliff");
    }

    #[test]
    fn unasserted_facts_are_neutral() {
        let p = base();
        let mut tg = Toks::new();
        tokenize(b"Located in the United States.", &mut tg);
        annotate_units(&mut tg);
        let mut ta = Toks::new();
        tokenize(b"Hosted by Google LLC since 2009", &mut ta);
        annotate_units(&mut ta);
        let mut sa = crate::sets::EMPTY_SET;
        sa.fill(&ta);
        let (m, _) = fact_multiplier(&ta, &tg, &sa, &p);
        assert_eq!(m, 1.0);
    }
}

//! Every tunable constant, in one block.
//!
//! Kept together deliberately: these are meant to be *swept* by the harness
//! iterate loop, not guessed, and a reviewer reading this file should be able to
//! see the whole decision surface at once. `tune.md` documents each one's
//! default and rationale. One source is compiled once per intent (A6); the
//! per-intent blocks below are selected by cargo feature.

#![allow(dead_code)]

/// Intent tag, for provenance. Not read by the node; mirrors the champion's
/// `TELEGRAPH_INTENT` marker so a reviewer can tell two builds apart.
#[no_mangle]
pub static TELEGRAPH_INTENT: [u8; 32] = intent_tag();

pub struct Profile {
    // ---- salience weights (A3.2) --------------------------------------
    /// Weight of a numeric token. Figures carry Tier-A correctness.
    pub w_number: f32,
    /// Weight of an identifier (IP, CVE id, version, date, coordinate).
    pub w_ident: f32,
    /// Weight of a stopword. Near zero, but not zero: stopwords still dilute a
    /// stuffed answer's precision denominator.
    pub w_stop: f32,
    /// Weight of an opaque non-Latin token (we cannot segment it).
    pub w_high: f32,
    /// Base weight of a content word, before the length bonus.
    pub w_word_base: f32,
    /// Extra weight per character of a content word, capped at `w_len_cap`.
    pub w_len_step: f32,
    pub w_len_cap: f32,
    /// Bonus for a mid-sentence capitalised token (proper nouns carry answers).
    pub w_proper: f32,

    // ---- anti-parrot (A3.6) -------------------------------------------
    /// Reserved. Question-echoed tokens are **not** discounted in precision:
    /// measured over 554 real rows, question-overlap correlates *negatively*
    /// (-0.258) with the champion's score, so a general echo penalty buys
    /// nothing and costs Spearman agreement. The echo flag is used only as a
    /// boolean inside the answered-ness gate, which is what actually catches
    /// the parrot.
    pub echo_discount: f32,

    // ---- answered-ness gate (A3.6, A3.9) ------------------------------
    /// Novel-supported-mass at which the answered-ness gate is fully open.
    pub ans_sat: f32,
    /// Fraction of the ground truth's own answer-bearing mass used as the
    /// saturation point when the GT is thin.
    pub ans_gt_frac: f32,
    /// Floor on the saturation point, so a one-word GT cannot open the gate on
    /// noise.
    pub ans_sat_min: f32,
    /// A token must weigh at least this much to count as decisive content.
    pub decisive_min: f32,
    /// How much ordinary prose counts toward *novelty*, relative to a hard
    /// assertion. Low, because a parrot padded with generic filler otherwise
    /// earns novelty credit whenever the ground truth is long enough to contain
    /// the same common words.
    pub novel_prose_w: f32,
    /// Prose novelty weight when the ground truth *does* state decisive facts.
    /// Much smaller: prose agreement is not assertion. Not zero, because a
    /// genuine prose-only answer must still outrank a question echo, whose
    /// tokens are excluded from novelty altogether.
    pub novel_prose_w_gt: f32,
    /// Floor under the answered-ness gate. Keeps a shut gate from collapsing
    /// every non-answer onto exactly the same value, so the ordering *below* the
    /// gate is still resolved by precision. Ties are what cost Spearman.
    pub ans_floor: f32,
    /// Below this much answer-bearing mass, the ground truth is itself
    /// refusal-shaped or hedged. Nothing can be "unanswered" against it, so the
    /// gate opens fully rather than zeroing every answer. In real traffic the
    /// refusals are usually the ground truths, not the answers.
    pub gt_decisive_min: f32,

    // ---- fact agreement (A3.4) ----------------------------------------
    /// Relative-error decay for a numeric near-miss: agreement = 1/(1 + k*rel).
    pub num_rel_k: f32,
    /// Relative tolerance inside which two figures are the same claim.
    pub num_rel_tol: f32,
    /// Absolute tolerance for bounded [0,1] quantities (risk scores, fractions).
    pub num_abs_tol: f32,
    /// For two *unitless* figures, how many multiples apart they may be and
    /// still count as claims about the same quantity. Beyond it the answer's
    /// figure is unverifiable rather than wrong. Figures carrying units are
    /// compared by dimension instead and ignore this.
    pub num_band_rel: f32,
    /// How much the *worst* figure in the answer, rather than the average one,
    /// decides the numeric channel. 0 = plain mean, 1 = worst figure only. A
    /// wrong decisive fact must not hide behind four right ones.
    pub num_min_bias: f32,
    /// How far each channel may pull the fact term down: 1.0 lets a wholly-wrong
    /// channel zero it, 0.0 disables the channel. Channels multiply.
    pub num_channel_w: f32,
    pub id_channel_w: f32,
    /// How much the *worst* identifier decides the identifier channel rather
    /// than the average one. Mirrors `num_min_bias` and `ent_min_bias`.
    pub id_min_bias: f32,
    /// Multiplier on a figure whose unit we could not identify, when the ground
    /// truth named a real one. Calibrated so that asserting a category error
    /// ("47 bananas") scores no better than asserting an honest wrong value
    /// ("47 m/s" where the truth is 47 km/h), which lands near 0.046.
    pub m_foreign_unit: f32,
    /// Multiplier on a bare figure matched against a united one. Weaker
    /// evidence than a properly-united match, but a legitimate shape
    /// (`wind_kmh=128.7`), so only a light discount.
    pub m_bare_unit: f32,
    /// Weight given to a flipped polar verdict ("plagiarised" vs "original").
    /// Carried on its own axis because a verdict word is neither an entity nor a
    /// figure, so without this it is charged at `prose_w` and a flip is free.
    pub verdict_w: f32,
    /// Multiplier applied when the answer asserts the OPPOSITE verdict to the
    /// one the ground truth states. Categorical: a flipped finding is wrong
    /// however much surrounding detail is right.
    pub m_verdict_flip: f32,
    /// How hard a polarity flip on supported content is punished. A sentence and
    /// its negation are different claims, not near-matches.
    pub m_contra: f32,
    /// How much the *worst* entity decides the entity channel, rather than the
    /// average one. Mirrors `num_min_bias`: a single swapped city must not hide
    /// behind five correct entities.
    pub ent_min_bias: f32,
    /// How far a wrong entity may pull the score down. 1.0 lets a wholly-wrong
    /// entity set zero the term; 0.0 disables the channel.
    pub ent_channel_w: f32,
    /// How hard a hyphenated range is discounted for its own width, relative to
    /// the figure it is compared against. A range that contains the truth is
    /// right; a range wide enough to contain any outcome is a hedge.
    pub m_range_width: f32,
    /// Floor of the fact multiplier. Keeps a wholly-wrong-figure answer above a
    /// cliff so near-misses stay distinguishable from garbage.
    pub fact_floor: f32,
    /// What an unsupported assertion costs when it displaced nothing — the
    /// answer already covers every entity and identifier the ground truth names,
    /// and then asserts one more.
    ///
    /// 0.0 makes such an addition free, which is the pure precision-of-answer
    /// reading (A3.8) and how this scorer behaved until it was measured:
    /// appending a false IP, a false ASN, a false country or a false city to an
    /// otherwise perfect answer all scored >= 0.9999. That is a real hole — an
    /// answer can pad itself with invented facts at no cost.
    ///
    /// 1.0 would treat the addition as a substitution, which is the recall
    /// reading and punishes an answer for volunteering *true* detail the ground
    /// truth happens not to restate. Nothing in the text distinguishes the two:
    /// with no slot schema, an extra true city and an extra false city look
    /// identical. So this is deliberately small — enough that padding is not
    /// free, small enough that a correct, generous answer stays at the top of
    /// the range. The asymmetry we cannot resolve is recorded in the README.
    pub add_w: f32,

    // ---- prose vs assertion (A3.4) ------------------------------------
    /// Share of precision carried by ordinary prose rather than by decisive
    /// assertions. A3.4 makes fact agreement dominant and lexical overlap "only
    /// a low-weight tie-breaker for prose quality" - this is that weight.
    /// Keeping it low stops a correct-but-wordy answer being diluted below a
    /// terse wrong one purely for using more words.
    pub prose_w: f32,

    // ---- shaping and calibration (A3.7, A8 stddev) --------------------
    /// Blend between linear precision and concave `p*(2-p)`. 0 = linear.
    pub p_concave: f32,
    /// Smoothstep knots applied to the raw composite. Widening these is the
    /// primary lever on `score_stddev` (gate needs > 0.05).
    pub ss_lo: f32,
    pub ss_hi: f32,
}

pub const fn base() -> Profile {
    Profile {
        // Heavy: on an intent whose answer IS a verdict, this single token is
        // the finding. Measured -- flipped verdict 0.9999 -> see tune.md.
        verdict_w: 8.0,
        m_verdict_flip: 0.04,
        w_number: 3.0,
        w_ident: 3.4,
        w_stop: 0.05,
        w_high: 0.5,
        w_word_base: 1.0,
        w_len_step: 0.06,
        w_len_cap: 12.0,
        w_proper: 1.0,

        echo_discount: 0.25,

        ans_sat: 3.0,
        ans_gt_frac: 0.5,
        ans_sat_min: 0.9,
        decisive_min: 0.5,
        novel_prose_w: 0.35,
        novel_prose_w_gt: 0.0,
        ans_floor: 0.05,
        gt_decisive_min: 0.8,

        num_rel_k: 8.0,
        num_rel_tol: 0.005,
        num_abs_tol: 0.02,
        num_band_rel: 10.0,
        num_min_bias: 0.5,
        num_channel_w: 0.9,
        id_channel_w: 0.9,
        id_min_bias: 0.6,
        m_foreign_unit: 0.05,
        m_bare_unit: 0.85,
        m_contra: 0.85,
        m_range_width: 2.0,
        ent_min_bias: 0.6,
        ent_channel_w: 0.9,
        fact_floor: 0.10,
        add_w: 0.35,

        // Unsupported prose is very nearly free. Prose the ground truth does not
        // restate is neither a decisive fact nor a contradiction, so it is not
        // evidence of a wrong answer, and none of the three anti-gaming channels
        // depends on it: parroting is caught by the answered-ness gate (novel
        // *supported* mass), wrong facts by the multiplicative fact/entity term,
        // contradictions by the polarity term. At the old 0.25 a *correct*
        // answer lost 12 points for wording the truth differently -- verbatim
        // 1.0000, reworded 0.8785 -- which is what cost registration 1377 the
        // ordering on the node's clean fixtures. Not literally zero, so padding
        // an answer with filler still dilutes it slightly. STORM_ALERT overrides
        // this back up; see the block below and tune.md for that trade.
        prose_w: 0.02,

        p_concave: 0.5,
        // Knots deliberately short of 0 and 1: clipping either end piles real
        // answers onto identical scores, and ties are what cost Spearman.
        ss_lo: 0.02,
        ss_hi: 0.92,
    }
}

// --------------------------------------------------------------------------
// Per-intent overrides. Only the constants that differ are restated, so the
// diff against `base()` *is* the per-intent tuning record.
// --------------------------------------------------------------------------

#[cfg(feature = "ip-geolocation")]
pub const fn profile() -> Profile {
    let mut p = base();
    // The IP itself is always echoed from the question, so the decisive content
    // is country/city/ISP/coordinates only. Demand real novel mass before the
    // answered-ness gate opens.
    p.ans_sat = 3.5;
    // Identifiers (the IP, the CIDR range, the AS number) are the spine of this
    // intent and admit no tolerance at all, so the identifier channel gets full
    // authority to zero the fact term.
    p.w_ident = 4.0;
    p.id_channel_w = 1.0;
    // Single miner means Spearman is skipped (A6), so this build can calibrate
    // for separation rather than for agreement with the champion's ordering.
    //
    // The ceiling is NOT pulled below 1.0 to buy margin. At ss_hi = 0.88 the
    // concave shaping mapped every precision at or above 0.800 to a literal 1.0,
    // so a wrong city, a wrong ISP and a wrong country each scored a perfect
    // 1.0000 while a correctly-reworded answer scored 0.9606 (pre-flight repro).
    p.ss_lo = 0.0;
    p.ss_hi = 1.0;
    // Concave shaping compounded it, lifting 0.80 to 0.96 before the smoothstep
    // saw it. Keep precision closer to linear so the top of the range ranks.
    p.p_concave = 0.15;
    // `prose_w` is the base 0.02 -- the fix that this intent's rejection
    // (registration 1377) paid for. Left in `base()` rather than restated here
    // because the finding is general: only STORM_ALERT, which must agree with a
    // lexical incumbent to clear Spearman, overrides it. Measured on this
    // profile: every correct phrasing >= 0.999 (was 0.8785 reworded) while a
    // wrong city, a wrong ISP and a swapped country all moved DOWN.
    p
}

#[cfg(feature = "storm-alert")]
pub const fn profile() -> Profile {
    let mut p = base();
    // Wind speeds and gusts arrive in m/s, km/h and knots across miners; the
    // unit normaliser handles the conversion, so the numeric channel is the
    // dominant signal here and deserves a tighter near-miss decay.
    p.num_channel_w = 1.0;
    p.num_rel_k = 10.0;
    // Risk is a bounded [0,1] score, so it wants an absolute epsilon. Held at
    // the base 0.02 rather than the 0.05 used before: on a canonical percentage
    // 0.05 is five whole points, which made a 1-point and a 5-point miss both
    // perfect and then dropped 73% of the score at 5.001 (review M4).
    p.num_abs_tol = 0.02;
    // ~4 miners means Spearman IS enforced. Full-range knots, so nothing is
    // clipped and every distinct composite keeps a distinct score: ties are
    // exactly what costs Spearman.
    p.ss_lo = 0.0;
    p.ss_hi = 1.0;
    // Ground truths for this intent are frequently themselves refusals, so the
    // answered-ness gate scales down with the GT's own thin content.
    p.ans_gt_frac = 0.40;
    // Prose carries most of precision here -- the Spearman tax, since the
    // incumbent ranks real traffic lexically. Not pushed to 1.0: that drops the
    // decisive-fact pool out of precision entirely and misranks correct answers
    // on questions unlike the tuning set.
    p.prose_w = 0.7;
    // The answered-ness gate is left almost closed. An earlier build set this to
    // 0.75, which pinned it open and paid a miner MORE for parroting the
    // question than for answering it: a mechanical echo scored 0.64 on recorded
    // rows against 0.03 for the real miner answers, and beat every recorded
    // answer on all 13 of them (review C3).
    p.ans_floor = 0.05;
    p.ans_sat = 6.0;
    // Prose novelty is not zeroed here as it is on IP_GEOLOCATION. Zeroing it
    // flattens the many prose-only recorded answers onto ~0, which destroys the
    // rank information the Spearman check reads. This value is the measured
    // maximum of that check subject to BOTH anti-gaming constraints still
    // holding (echo 0.0049 and field-name blob 0.0029, against a recorded-answer
    // mean of 0.0152). See tune.md: the check still does not pass.
    p.novel_prose_w_gt = 0.12;
    p
}

#[cfg(feature = "content-verification")]
pub const fn profile() -> Profile {
    let mut p = base();
    // Plagiarism / authenticity checking. The decisive content is a polar
    // verdict (plagiarised vs original, AI-generated vs human), a similarity
    // percentage, and the matched source. All three are things an answer either
    // gets right or gets backwards, so this profile leans on the polarity and
    // identifier channels rather than on prose.
    //
    // A verdict flip is the characteristic wrong answer here, and it shares
    // almost all its vocabulary with the correct one ("the text IS original" vs
    // "the text is NOT original"), so the contradiction path must dominate.
    p.w_ident = 4.0;
    p.id_channel_w = 1.0;
    // Similarity is a bounded percentage: an absolute epsilon, not a relative
    // one, or "12%" and "82%" both read as near-misses of a 47% truth.
    p.num_abs_tol = 0.02;
    // Steep. A similarity percentage is the finding, not a measurement with
    // tolerance: reporting 21% against a truth of 68% is a wrong answer, not a
    // near miss. Swept 10/25/60/150 -- margin 0.8668/0.8754/0.8793/0.8812, so
    // this is the knee. 150 was rejected as over-punishing: it makes a 1% error
    // look like a 100% one, which would misrank genuinely close answers on
    // questions unlike the tuning set.
    // Worst figure decides the numeric channel outright. On this intent every
    // figure IS the finding -- the similarity percentage and the passage count
    // are what a plagiarism report exists to state -- so one wrong figure must
    // not average away behind the right ones. Swept 0.5(base)/0.6/0.85/1.0 ->
    // margin 0.8793/0.9014/0.9463/0.9634, correct answers unmoved at 0.9999
    // throughout, so this buys separation at no cost to correctness.
    //
    // The entity mirror (`ent_min_bias`) was swept too and left at the base 0.6:
    // raising it to 0.8/1.0 REDUCED margin (0.9543/0.9483), because a wrong
    // source is one entity among several correct ones and worst-biasing it
    // dragged the whole entity channel rather than sharpening it.
    p.num_min_bias = 1.0;
    p.num_rel_k = 60.0;
    p.num_channel_w = 1.0;
    // Single miner with no scoring history (historical_rows_evaluated: 0), so
    // the Spearman traffic check is SKIPPED for this intent. That frees this
    // build to calibrate for separation outright, exactly as IP_GEOLOCATION
    // does, without the agreement tax that STORM_ALERT pays.
    p.ss_lo = 0.0;
    p.ss_hi = 1.0;
    p.p_concave = 0.15;
    // The matched source and the verdict are the answer; a fluent restatement
    // of the submitted passage is not. Demand real novel mass before the
    // answered-ness gate opens.
    p.ans_sat = 3.5;
    p
}

#[cfg(feature = "text-authenticity")]
pub const fn profile() -> Profile {
    let mut p = base();
    // TEXT_AUTHENTICITY_CHECK. Same question as content verification -- is this
    // text original, AI-generated or human-written -- so it shares the profile:
    // a polar verdict, a confidence/similarity figure, and a named source. The
    // antonym axis (src/antonyms.rs) already carries this vocabulary.
    //
    // Zero miners with scoring history (historical_rows_evaluated: 0 on the
    // champion and on every recent challenger), so Spearman is SKIPPED here too
    // and the profile can calibrate for separation outright. The decisive content is a polar
    // verdict (plagiarised vs original, AI-generated vs human), a similarity
    // percentage, and the matched source. All three are things an answer either
    // gets right or gets backwards, so this profile leans on the polarity and
    // identifier channels rather than on prose.
    //
    // A verdict flip is the characteristic wrong answer here, and it shares
    // almost all its vocabulary with the correct one ("the text IS original" vs
    // "the text is NOT original"), so the contradiction path must dominate.
    p.w_ident = 4.0;
    p.id_channel_w = 1.0;
    // Similarity is a bounded percentage: an absolute epsilon, not a relative
    // one, or "12%" and "82%" both read as near-misses of a 47% truth.
    p.num_abs_tol = 0.02;
    // Steep. A similarity percentage is the finding, not a measurement with
    // tolerance: reporting 21% against a truth of 68% is a wrong answer, not a
    // near miss. Swept 10/25/60/150 -- margin 0.8668/0.8754/0.8793/0.8812, so
    // this is the knee. 150 was rejected as over-punishing: it makes a 1% error
    // look like a 100% one, which would misrank genuinely close answers on
    // questions unlike the tuning set.
    // Worst figure decides the numeric channel outright. On this intent every
    // figure IS the finding -- the similarity percentage and the passage count
    // are what a plagiarism report exists to state -- so one wrong figure must
    // not average away behind the right ones. Swept 0.5(base)/0.6/0.85/1.0 ->
    // margin 0.8793/0.9014/0.9463/0.9634, correct answers unmoved at 0.9999
    // throughout, so this buys separation at no cost to correctness.
    //
    // The entity mirror (`ent_min_bias`) was swept too and left at the base 0.6:
    // raising it to 0.8/1.0 REDUCED margin (0.9543/0.9483), because a wrong
    // source is one entity among several correct ones and worst-biasing it
    // dragged the whole entity channel rather than sharpening it.
    p.num_min_bias = 1.0;
    p.num_rel_k = 60.0;
    p.num_channel_w = 1.0;
    // Single miner with no scoring history (historical_rows_evaluated: 0), so
    // the Spearman traffic check is SKIPPED for this intent. That frees this
    // build to calibrate for separation outright, exactly as IP_GEOLOCATION
    // does, without the agreement tax that STORM_ALERT pays.
    p.ss_lo = 0.0;
    p.ss_hi = 1.0;
    p.p_concave = 0.15;
    // The matched source and the verdict are the answer; a fluent restatement
    // of the submitted passage is not. Demand real novel mass before the
    // answered-ness gate opens.
    p.ans_sat = 3.5;
    p
}

#[cfg(all(
    feature = "generic",
    not(feature = "ip-geolocation"),
    not(feature = "storm-alert"),
    not(feature = "content-verification"),
    not(feature = "text-authenticity")
))]
pub const fn profile() -> Profile {
    base()
}

const fn intent_tag() -> [u8; 32] {
    #[cfg(feature = "ip-geolocation")]
    let name = b"IP_GEOLOCATION";
    #[cfg(feature = "storm-alert")]
    let name = b"STORM_ALERT";
    #[cfg(feature = "content-verification")]
    let name = b"CONTENT_VERIFICATION";
    #[cfg(feature = "text-authenticity")]
    let name = b"TEXT_AUTHENTICITY_CHECK";
    #[cfg(all(
        feature = "generic",
        not(feature = "ip-geolocation"),
        not(feature = "storm-alert"),
        not(feature = "content-verification"),
        not(feature = "text-authenticity")
    ))]
    let name = b"GENERIC";

    let mut out = [0u8; 32];
    let mut i = 0usize;
    while i < name.len() && i < 32 {
        out[i] = name[i];
        i += 1;
    }
    out
}

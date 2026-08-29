# scorer — a fact-aware Telegraph scoring module

[![Scorer CI](https://github.com/shreshth006/telegraph-factscore/actions/workflows/ci.yml/badge.svg)](https://github.com/shreshth006/telegraph-factscore/actions/workflows/ci.yml)

A freestanding `wasm32-unknown-unknown` scoring module, ~19.6 KB, **zero imports**, no allocator,
no clock, no randomness, no transcendental maths. One Rust source tree compiled once per intent
via constant profiles, the same shape the incumbent uses.

**Scoring in one line:** weight each token by how much it decides the answer, measure how much of
what the *answer asserts* the ground truth supports, gate on whether the answer said anything the
question did not already give away, multiply by typed agreement on figures and identifiers, then
calibrate with a smoothstep instead of a step.

---

## Why this is different from the incumbent

The text actually scored is `converted_answer` — a flat third-person summary, 86.9% of which opens
literally `"The data …"`, and a median **2.25× shorter** than the markdown ground truth
(measured over 515 public score records). Any scorer built on symmetric overlap or on
*recall of the truth* is structurally penalised, which is why live medians sit near 0.006.

So this module scores **precision of the answer**: of what the answer asserts, how much does the
ground truth support? Four consequences fall out, and each is a measurable improvement over the
incumbent rather than a stylistic preference:

1. **Typed facts decide.** Figures are compared inside a *dimension* after unit normalisation —
   `18 km/h` and `5 m/s` are the same claim, `55%` and `0.55` are the same claim, and a
   temperature is never a near-miss for a wind speed. Identifiers (IPs, CVE ids, versions, dates,
   coordinates) admit no tolerance at all.
2. **A wrong *entity* is a contradiction, not a rounding error.** A swapped city, ISP or country
   goes through the same multiplicative channel as a swapped figure, worst-case-leaning so one
   wrong entity cannot hide behind five right ones. The guard is a pairing rule: an unsupported
   entity only counts as a *substitution* to the extent the ground truth names entities the answer
   never mentions, so extra true detail stays neutral. Legitimate variation survives — a run of
   proper nouns is also indexed by its acronym, so `US` matches `United States` with no synonym
   table, and a **two-letter** code the answer uses instead of a name (`UY` for Uruguay, which no
   lexical rule reaches) abstains rather than counting as a claim. The exemption is bounded at two
   letters on purpose: it used to cover every ALL-CAPS token, and a wrong ISP written `AWS` scored
   **0.9829** where the same swap spelled `Cloudflare Inc.` scored 0.2248.

3. **A wrong fact is hard to hide.** Channels combine multiplicatively and the numeric channel
   leans on its *worst* figure, so quoting the right CVE id does not rescue a wrong CVSS score, and
   a polarity flip on supported content is scored as contradiction rather than coverage. Measured
   against the ground truth *"8.8.8.8 is located in Mountain View, California, United States,
   operated by Google LLC"*: swapping **one** entity (city → Berlin) scores **0.3047**, swapping
   three (city, state, country) scores **0.0514**, and the wrong ISP alone scores **0.2030** —
   against a verbatim-correct 1.0000 and a *reworded*-correct **0.9992**. Legitimate variation is
   untouched: `US` for `United States` scores **0.9992**, appending true detail the ground truth
   never mentions scores **0.9999**, and adding one true identifier the truth omits (`AS15169`)
   scores **1.0000**.

   That reworded figure used to be 0.8785, and it is what cost registration 1377 its promotion:
   the node's benchmark is **clean good-vs-bad pairs**, where the incumbent is lexically generous
   and any recognisably-correct answer earns ~1.0, so charging a correct answer for its own wording
   is the whole gap. See `tune.md`, "The clean-pair round", for the five mechanisms and their
   before/after.

   An earlier build of this module failed exactly here — a lone swapped city scored a perfect
   1.0000, tying a verbatim-correct answer. It was caught by probing the *hosted binary* rather
   than the fixture corpus, which had never tested a single-entity swap. The `ENTITY-SWAP` fixture
   class (18 cases) exists so that gap cannot reopen.
4. **Unasserted facts are neutral.** A figure the ground truth never discusses is unverifiable,
   not wrong, so a terse-but-correct answer is not punished for omission. Measured on the review's
   own strings: terse-correct 1.0000, verbose-correct 0.9994, JSON-formatted correct 0.9991,
   reordered correct 0.9995. An unsupported figure the truth never discusses stays neutral, and so
   does an unsupported identifier — but only to the extent the answer has already covered the
   identifiers the truth *does* name. That pairing rule is what separates "true but unrestated"
   from "substituted"; without it, an answer that quoted the right IP and added the right AS
   number scored 0.4876.
5. **Answered-ness is first-class.** After the boilerplate opener is struck, an answer that
   asserts nothing beyond the question's own content scores near zero — *when the ground truth
   carries an answer to be found*.
6. **Notation is not content.** Three normalisations exist because each one was measured punishing
   a *correct* answer for how it was typed rather than what it said. Unicode punctuation folds to
   ASCII before tokenising — bytes ≥ 0x80 are opaque, so `Shimo’ochiai` with a curly apostrophe and
   the same name with `'` were different tokens, and the ASCII form scored **0.2592** against a
   curly ground truth (now 0.9998). A lone hemisphere letter is read as a coordinate sign before
   the unit table sees it, because `s`, `n` and `e` all name units and only `W` had ever worked:
   `34.9011 S, 56.1645 W` scored **0.2055** against a signed ground truth (now 1.0000), while
   `30 s` is still a duration and `47 bananas` still 0.0005. ISO 3166 alpha-2 codes are indexed in
   both directions, so `UY` is Uruguay — and, unlike the blanket abstention it replaced, a *wrong*
   two-letter country code now costs (0.97 → 0.2796).

### What the module deliberately does **not** do

- **It does not penalise question-vocabulary overlap.** Measured across 554 real rows, bag-of-words
  overlap with the question correlates *negatively* (−0.258) with the incumbent's score: the parrot
  effect is positional, not an overlap effect. A general echo penalty would buy nothing and would
  wreck the Spearman agreement the gate requires. The echo flag is used only as a boolean inside
  the answered-ness gate.
- **It never relitigates the ground truth.** In real traffic the refusals are usually the *ground
  truths*, not the answers (8 of 15 weather GTs are hedged; 40 of 58 sub-0.02 rows). When the
  ground truth is itself refusal-shaped, a hedged answer is the correct answer and the gate opens
  fully rather than zeroing everything.
- **No embedding in the hot path.** The whole pipeline is integer and `f32` arithmetic —
  `core` has no `powf`/`exp`/`sqrt` and pulling in libm would add host imports. It also means the
  full fixture gate projects to ~10 s of the node's 600 s budget.
- **No miner fingerprints.** No slug, wallet, field name or phrasing is matched, favourably
  or otherwise. The `OUR-STYLE-WRONG` fixture class exists to prove it: a `livecert`-shaped answer
  with wrong facts loses to a competitor-shaped answer with right facts, 1/1 on both intents.
- **It does not charge a correct answer for prose the ground truth omits.** `prose_w` is 0.02:
  unsupported prose that is neither a decisive fact nor a contradiction is very nearly free. That
  is safe because none of the anti-gaming properties rests on it — parroting is caught by the
  answered-ness gate (novel *supported* mass), wrong facts by the multiplicative fact and entity
  channels, contradictions by the polarity term. Measured after the change, every wrong answer in
  the regression set scored the same or **lower**, while every correct rephrasing reached ≥0.999.
  It is not literally zero, so padding still dilutes slightly.

## Pipeline

```
rank_answer(q, gt, ma)
  ├─ ma blank (empty or ASCII whitespace)      -> EXACTLY 0.0     [Stage-1 trap]
  ├─ normalized_equal(gt, ma)                  -> EXACTLY 1.0     [self-match ratchet]
  └─ tokenise -> annotate units -> mark boilerplate / echo / support
       P     precision of assertion   decisive facts, plus prose at prose_w
       ans   answered-ness gate       novel supported mass, conditioned on the GT
       fmul  typed fact agreement     numbers graded, identifiers exact, multiplicative
       ent   entity agreement       proper nouns, multiplicative, substitution-paired
       pol   polarity               a flip on supported content is a contradiction
       raw   = shaped(P) * fmul * ent * ans * pol
       score = smoothstep(ss_lo, ss_hi, raw)
```

`ent`, the identifier half of `fmul`, and the decisive pool of `P` all read the same
**substitution-versus-addition** rule: an unsupported entity or identifier only counts against the
answer to the extent the ground truth names ones the answer never mentions. An answer that says
everything the truth says and then says more has contradicted nothing (A3.8).

Note the `normalized_equal` short-circuit above: a **verbatim** answer never reaches the pipeline.
That is why four separate defects that only ever charged *reworded* correct answers survived every
earlier test round, and why the `CLEAN-PAIR` fixture class — which rewords each ground truth four
ways — exists.

Every constant is documented in [tune.md](tune.md) and lives in one block in
[`src/profile.rs`](src/profile.rs), so a reviewer sees the whole decision surface at once.

| File | Purpose |
|---|---|
| `src/lib.rs` | The ABI: `alloc`, `dealloc`, `rank_answer`, `breakdown_answer`, and the Stage-1 traps |
| `src/bytes.rs` | Byte classes, case folding, FNV-1a hashing, smoothstep, float helpers |
| `src/tokens.rs` | Tokenisation, salience weights, boilerplate openers |
| `src/units.rs` | Figure parsing, unit tables, dimensions, normalisation |
| `src/facts.rs` | Graded agreement and the multiplicative fact term |
| `src/sets.rs` | Open-addressed token sets (matching stays linear, not n·m) |
| `src/score.rs` | The pipeline above |
| `src/profile.rs` | Every tunable constant, and the per-intent overrides |

## Build

```bash
export PATH="/c/Users/hyada/.cargo/bin:$PATH"

cargo test                                                     # 58 unit tests, host target
cargo build --release --target wasm32-unknown-unknown          # generic
cargo build --release --target wasm32-unknown-unknown --no-default-features --features ip-geolocation
cargo build --release --target wasm32-unknown-unknown --no-default-features --features storm-alert

wasm-tools print target/wasm32-unknown-unknown/release/scorer.wasm | grep -c '(import'   # must be 0
wasm-tools validate target/wasm32-unknown-unknown/release/scorer.wasm
node verify.mjs dist/ip_geolocation.wasm
```

GitHub Actions independently repeats formatting, clippy, three profile test
suites, a freestanding WASM build, the tracked hybrid artifact's SHA-256 and
adversarial verifier, and the public IP fixture gate. The workflow fails if
self-match, score spread, absolute separation, or any ordered public pair
regresses.

The crate is `#![no_std]` on wasm and links `std` on the host, so `cargo test` runs normally while
the shipped artefact stays freestanding. `wasm-tools print | grep -c '(import'` is the check that
proves it — a WASI or `wasm-bindgen` build is an instant registration reject.

## Verification

All three builds pass `verify.mjs` in full. Artefacts:

| Build | Size | Imports | `wasm-tools validate` |
|---|---|---|---|
| `dist/generic.wasm` | 22,482 B | **0** | OK |
| `dist/ip_geolocation.wasm` | 22,486 B | **0** | OK |
| `dist/storm_alert.wasm` | 22,494 B | **0** | OK |

`cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` are both clean; 66 unit tests
pass under the generic and IP profiles, 61 under STORM (five assertions are gated off there, where
`prose_w = 0.7` deliberately changes the behaviour they pin).

Exported signatures, read back off the binary — `rank_answer` is **exactly six `i32` returning
`f32`** (a 3-param build was rejected live):

```
(export "alloc"            (func (param i32) (result i32)))
(export "dealloc"          (func (param i32 i32)))
(export "rank_answer"      (func (param i32 i32 i32 i32 i32 i32) (result f32)))
(export "breakdown_answer" (func (param i32 i32 i32 i32 i32 i32 i32) (result i32)))
```

Stage-1 traps, each reproducing a recorded live rejection:

```
PASS  zero imports (freestanding)  []
PASS  rank_answer takes exactly 6 params  got 6
PASS  empty answer is EXACTLY 0.0  got 0            <- host passes ptr=0,len=0 without calling alloc
PASS  whitespace-only (spaces) is EXACTLY 0.0  got 0   <- a returned 0.0007 failed a live registration
PASS  self-match beats unrelated cross-match  1 > 0
PASS  self-match clears the 0.75 ratchet  1
PASS  ~54 KB repeated text does not trap  0
PASS  emoji/CJK/accents do not trap
PASS  allocator never returns 0 under sustained load
```

Hand cases on the `ip_geolocation` build:

```
  1.000000  self-match (ground truth as answer)
  0.999275  correct + terse
  1.000000  correct as JSON              <- format equivalence: JSON scores like prose
  0.000070  wrong location (fact swap)
  0.002215  question echo (contentless)  <- the incumbent scores this 0.9933
  0.001962  content-filter refusal
  0.018832  keyword stuffing
  0.000000  off-topic
  0.000000  empty

  The CVSS score is 1.0   (truth: 10)   -> 0.0018   <- a punctuation regroup is
  The IP is 192.168.11.0  (truth: .1.10) -> 0.0074     not an "exact match"
  ... is NOT located in Germany          -> 0.0611   <- polarity is read
  47 bananas / 47 hPa   (truth 47 km/h)  -> 0.0005 / 0.0076  <- category errors

  CVSS 10   -> 1.0000     CVSS 9.7  -> 0.9877     CVSS 9.0 -> 0.7695
  CVSS 9.9  -> 1.0000     CVSS 9.5  -> 0.9340     CVSS 3.1 -> 0.2330
  (monotone the whole way down: support is graded, not a threshold)

  5 m/s   (same unit)   -> 1.000000
  18 km/h (same speed)  -> 1.000000      <- unit normalised
  47 m/s  (wrong speed) -> 0.002409
```

## Measured against the live champions

`track2/harness/run-eval.mjs` reproduces the node's two-stage gate offline against the incumbent
binaries (`ipgeo_reg630`, `storm_rpen_reg453`).

> **The table below is superseded.** It predates registration 1377's rejection, the repaired
> CLEAN-PAIR corpus, and — most importantly — the discovery that IP_GEOLOCATION's public history
> now has two miners, so check C is **not** skipped there. See "IP_GEOLOCATION is no longer
> Spearman-free" under *Honest limitations*, and `tune.md` for the current numbers.

Current IP_GEOLOCATION figures against `ipgeo_reg630`, on the repaired corpus:

| | candidate | incumbent |
|---|---|---|
| pairwise wins, whole corpus | **786/791** | 485/791 |
| margin | **0.7221** | 0.2934 |
| CLEAN-PAIR (fluent one-fact counterfactuals) | **744/744** | 454/744 (61.0%) |
| ENTITY-SWAP | **18/18** | 9/18 |
| UNIT/FORM | **4/4** | 2/4 |
| REAL-PARROT | 3/8 | 4/8 |
| correct-form spread ≤ 0.05 | **31/31** (worst 0.0280) | 31/31 |

`REAL-PARROT` is the one class where the incumbent still leads, and it is the class the public
story used to lead with. It is reported, not hidden.

| Check | IP_GEOLOCATION | STORM_ALERT |
|---|---|---|
| A stddev > 0.05 | PASS 0.4218 | PASS 0.4155 |
| B self-match ≥ max(0.75, incumbent) | PASS 1.0 vs bar 1.0 | PASS 1.0 vs bar 0.9933 |
| C Spearman ≥ 0.60 | SKIP (1 miner) | **PASS 0.6005** |
| D1 margin > champion (strict) | PASS **0.814** vs 0.438 | PASS **0.804** vs 0.385 |
| D2 margin ≥ 0.15 | PASS | PASS |
| D3 wins ≥ champion | PASS 42/47 vs 31/47 | PASS 31/37 vs 26/37 |
| **Verdict** | **would promote** | **would promote** |

STORM_ALERT's Spearman is **0.6005 against a floor of 0.60** — it passes by 0.0005. Treat that as
unproven on the node's own fixtures, which are not this corpus: a small rotation could put it
either side. IP_GEOLOCATION, where the check is skipped entirely and the margin delta is +0.376,
is the safe first registration.

Check C was *failing* at 0.5926 before the entity channel was added; scoring
swapped entities as contradictions restored the rank information the check reads
on real traffic and moved it to 0.6005. The anti-gaming properties it was traded
against are all still in place — `tune.md` records the history.

Per-class pairwise ranking accuracy, candidate vs incumbent:

| Class | IP_GEO cand | IP_GEO ref | STORM cand | STORM ref |
|---|---|---|---|---|
| FACT-SWAP | **4/4** | 4/4 | **4/4** | 4/4 |
| UNIT/FORM | **4/4** | 2/4 | **4/4** | 2/4 |
| LENGTH | **2/2** | 1/2 | **2/2** | 1/2 |
| CONTRADICTION | 1/1 | 1/1 | **1/1** | 0/1 |
| ENTITY-SWAP | **18/18** | — | **8/8** | — |
| REAL-PARROT | 3/8 | 4/8 | **5/8** | 1/8 |
| OUR-STYLE-WRONG | 1/1 | 1/1 | 1/1 | 1/1 |
| REFUSAL / STUFFING / EMPTY / CONTENT-FILTER / TEMPORAL | all 1.0 | all 1.0 | all 1.0 | all 1.0 |

REAL-PARROT on IP_GEOLOCATION reads 3/8, below the incumbent's 4/8, and the
number is honest but misleading on its own: all five losses are ties at the noise
floor (real answer 0.00000–0.00196 against a parrot at ~0.0025, where the node
itself scored those same answers 0.005–0.046). They are rows where the recorded
miner answer is *itself* factually wrong — one claims Brisbane, Australia for a
private 192.168.1.10 — so scoring it at or below a contentless echo is the
precision-of-answer thesis working, not failing. On STORM_ALERT, where the echo
attack was the review's headline, the class moves 1/8 → 5/8.

The FACT-SWAP margins are the clearest exhibit: **0.458** (IP_GEO) and **0.505** (STORM) against
the incumbent's **0.004**. The incumbent orders those pairs correctly but by a margin four
thousandths wide — it is very nearly blind to a swapped decisive fact, which is exactly the failure
mode a Tier-A deterministic intent cannot tolerate.

## Honest limitations

- **The corpus is a proxy, not the node's benchmark.** The node's fixtures are closed-source and
  unrecoverable. What transfers is the *comparison* against a pinned incumbent binary,
  not the absolute numbers. One point of contact exists: on the CLEAN-PAIR class the incumbent
  measures margin **0.99210**, and the node reported that same incumbent at **0.99186** on its own
  hidden fixtures.
- **An appended false fact costs, but far less than a wrong one — and an appended *true* fact pays
  the same price.** Nothing in the text separates them without slot-aware extraction, which this
  module does not have. `add_w = 0.35` prices an unpaired assertion so padding is not free
  (a false extra IP 0.9999 → 0.8100, a false ASN 1.0000 → 0.8154), and the identical discount lands
  on a *correct* answer that volunteers a true AS number (1.0000 → 0.8154). Appended false
  countries and cities still cost under a point, because the entity channel is worst-case-leaning
  and a pure addition is not a contradiction. This is the sharpest unresolved limitation.
- **The CLEAN-PAIR headline was overstated until the generator was repaired.** Its wrong answers
  were positional substitutions over the whole text, which produced corrupted strings a scorer can
  reject on fluency alone. Rebuilt as fluent one-fact counterfactuals, the same build scores
  744/744 with margin **0.698** where the corrupted corpus reported 248/248 and 0.999. The lower
  number is the real one.
- **It does not beat the incumbent everywhere, and here is a case where it loses.** Run the
  *generic* build against `SSL_VERIFICATION` (champion reg 631) and it **fails the gate**: wins
  16/29 against the incumbent's 17/29, and Spearman **−0.2222** over 18 real answers — our ranking
  of that intent's live traffic is close to the *opposite* of the champion's. Two honest readings,
  and we cannot yet separate them: either the SSL champion encodes something about certificate
  answers that pure fact-precision misses, or SSL's recorded traffic is dominated by answers whose
  correctness our extractor cannot see (that intent has no per-intent extractor — the generic
  build has no notion of a chain, a SAN, or an expiry). Either way the claim we make is the narrow
  one: measured wins on `IP_GEOLOCATION` and `STORM_ALERT`, against pinned binaries, with the
  method published so the losses are as reproducible as the wins.
- **STORM_ALERT passes, but by 0.0005 — and the tension behind that number is the real finding.**
  The intent has ~4 miners, so the Spearman check (ρ ≥ 0.60 agreement with the incumbent's ranking
  of real traffic) is enforced — and the incumbent *rewards* contentless echoes there. Refusing to
  reward them costs agreement directly: a 72-build sweep over the storm profile hit a **ceiling of
  0.593**, and the module was written off as unpromotable on this intent. The entity-swap fix then
  lifted it to **ρ 0.6005** as a side effect, and it now clears all six checks (margin 0.804 vs
  0.385, wins 31/37).

  Both facts matter. The gate *is* passable, so "the agreement check makes the incumbent
  unassailable" would be too strong a claim. But clearing a floor by five ten-thousandths, on a
  proxy corpus, against hidden fixtures that rotate, is not a safety margin — and the direction of
  the pressure is real: every step away from rewarding parroting costs agreement with a scorer
  that rewards it. IP_GEOLOCATION, where the check is *skipped* outright, is the structurally safe
  registration; STORM is a cheap, informative second attempt.
- **IP_GEOLOCATION is no longer Spearman-free, and that now blocks registration.** Everything
  above and below that calls this intent "single miner, check skipped" is stale. Public history
  now carries 25 rows (13 scorable) across **two** miners, `iplocate` and `livecert`, so check C
  applies. Replayed against the live champion on that history this build scores **ρ 0.5934** per
  row and **0.6503** per distinct question, against a floor of 0.60. The rejected registration
  1377 scored 0.5824, so this round improved agreement *and* correctness — but not enough.

  It is not a tuning problem. All 13 scores are distinct on both sides, so there are no ties to
  break; the gap is concentrated on rows where the champion scores a **factually wrong** answer at
  ~0.99 — "located in Mumbai, India" against a ground truth of Tokyo, Japan (champion 0.9918, ours
  0.0855), and against a ground truth of the United States (champion 0.9960, ours 0.0156). Raising
  ρ means scoring those closer to the champion. The agreement check, on this intent's current
  traffic, asks a candidate to reproduce the incumbent's errors.

  We report this rather than tune around it. Registration is a judgement about whether to spend a
  gate attempt on a check we can only pass by getting answers wrong.
- **Tuning was measured, not guessed**, but only against this corpus. The sweep imports the
  harness's own `corpus.mjs` so the Spearman set optimised is byte-identical to the one the gate
  reads — an earlier sweep against a hand-rolled proxy reported ρ 0.639 where the harness measured
  0.538, which is precisely the error that makes a candidate fail on-chain after passing locally.
- **The top-end saturation is only partly fixed (review C4/C5).** The *mechanism* is gone: the
  ceiling no longer maps every precision at or above 0.800 to a literal 1.0, and a controlled
  fact-swap sweep that previously read `1.0000 / 1.0000 / 1.0000 / 0.9990` for 0/1/2/3 wrong facts
  now reads `1.0000 / 0.9985 / 0.9854 / 0.9597` — monotone, no ties. But two things the review
  measured have **not** improved:
  - **19 of 75 corpus answers still score exactly 1.0** (distinct values 41, down from 46). These
    are answers with literally perfect precision — every token they assert is supported — which
    under precision-of-answer genuinely *are* equivalent. It is a property of the thesis, not a
    calibration artifact, but it is the same tie mass, and if a second miner ever registers on
    IP_GEOLOCATION it is what check C would see (GAPS G12).
  - **The specific inversion is not fixed**: a 3-of-5-wrong answer that keeps the ASN and
    coordinates still scores 0.9597 against 0.8329 for a correct-but-hedged answer that omits them.
    Both answers carry roughly equal *unsupported decisive mass* (`Norway/Hordaland/Bergen` versus
    `WHOIS/RIR` plus hedging prose), so no bag-of-words measure separates them — the wrong answer is
    not "hiding" a fact so much as sharing more surface with the ground truth. Fixing it needs
    slot-aware alignment (which GT fact does this clause fill?), which this design does not have.
    A recall term was evaluated and rejected: the 3-of-5-wrong answer has *higher* decisive recall
    than the hedged correct one, so recall makes the inversion worse, not better.

- **Known residual limits**, each measured rather than asserted:
  - *Precision without recall cannot fully separate "true but unrestated" from "unsupported".* A
    correct verbose answer scores 0.9995 against a correct terse 1.0000, and a five-fact answer
    with three proper nouns swapped still reaches 0.976. Swapped *figures* are punished far
    harder than swapped *words*; a recall term would close this but would re-penalise the terse
    answers that dominate live traffic (A3.8).
  - *A bare figure and a figure in an unrecognised unit are lexically identical to the module.* The
    expanded unit table routes real units (hPa, kelvin, mb, psi, inHg) to their own dimensions, and
    an unrecognised unit-shaped word is discounted hard, but a nonsense unit still lands at 0.0005
    against an honest wrong value at 0.0004 — both effectively zero, where the pre-fix module
    scored the category error 0.97 against 0.015.
  - *Guessing the modal answer still scores well when the mode happens to be right.* No scorer can
    distinguish a lucky prior from knowledge on a single row.
- `breakdown_answer` is debug-only and is never called by either gate.

## Prior art and method

The incumbent champion — `zkasuran/telegraph-salience-scorer` (MIT) — was studied openly, both
its published source and its compiled behaviour, and several of its sound ideas (salience
weighting, a normalized exact-match short-circuit, multiplicative penalties for decisive-fact
disagreement) shaped this design. This module is an independent implementation, not a fork; where
the two disagree — precision-of-answer vs recall-of-truth, smoothstep vs step calibration, typed
unit normalisation, the answered-ness gate — the choice was made by measurement against public
score records and the incumbent's own binaries, and the reasoning is recorded in `tune.md`.

## Disclosure

The author of this scoring module also operates the Track 1 miner `livecert` (registration 225),
which serves intents including STORM_ALERT and IP_GEOLOCATION. The module encodes general intent
correctness — its test corpus includes cases where livecert's own answer style is scored **down**
when factually wrong (the `OUR-STYLE-WRONG` class) — and the overlap was proactively disclosed to
the hackathon organizers, who will flag it for transparent review. No slug, wallet, field name or
phrasing is matched by the scoring logic, favourably or otherwise.

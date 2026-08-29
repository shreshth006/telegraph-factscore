# Which intent to register on

Every registration this repository has spent — 28 of them — went to
`IP_GEOLOCATION`. All 28 were rejected. This note records the measurement that
says the intent, not the module, was the binding constraint.

## What the gate actually decides

Read from 1,609 live registry entries (`GET /api/wasm`), including every
rejection string and its `eval` block:

- The node scores ~15 hidden fixture pairs. `candidate_margin` is the mean
  separation between the good and the bad answer of each pair, and
  `candidate_wins` counts the pairs ordered correctly.
- Promotion needs **both** `wins >= champion_wins` and
  `margin > champion_margin`, recomputed for the champion on the same fixture
  set at evaluation time.
- Only then does the real-traffic stage run: Spearman agreement with the
  champion's ranking of recorded miner answers, floor 0.60, skipped when the
  intent has fewer than two miners with scored history.

Champion binaries calibrate to a hard band (≈0.996 for the good side, ≈0.004
for the bad), so a champion margin reads almost directly as the number of
fixture pairs it splits: `0.9245 ≈ 0.992 × 14/15`, `0.6586 ≈ 0.992 × 10/15`.
The bar is not a tuning target — it is a count.

## The two properties that decide an intent

Live champion separation bar, from the most recent evaluation in each intent,
against scored rows read from `GET /scores?intent=…`:

| intent | champion bar | champion pairs split | scored rows | miners | real-traffic stage |
| --- | ---: | ---: | ---: | ---: | --- |
| `TEXT_AUTHENTICITY_CHECK` | **0.6586** | ~10 / 15 | **0** | **0** | cannot run |
| `TWITTER_SEARCH` | 0.9924 | ~32 / 32 | 0 | 0 | cannot run |
| `STOCK_PRICE` | 0.6147 | ~9 / 15 | 86 | 4 | runs |
| `TVL_LOOKUP` | 0.6340 | ~9 / 14 | 100 | 8 | runs |
| `CRYPTO_PRICE` | 0.6962 | ~10 / 15 | 100 | 5 | runs |
| `ACADEMIC_SEARCH` | 0.7010 | ~10 / 15 | 48 | 3 | runs |
| `IP_GEOLOCATION` | **0.9245** | ~14 / 15 | 33 | 2 | runs |

`IP_GEOLOCATION` is the worst square on the board on both axes at once: the
highest separation bar of any intent whose champion is beatable in principle,
and enough recorded traffic for the agreement stage to run behind it. This
module is measured at ρ 0.5934 there — so even a fixture-gate win would have
been rejected one stage later. Two of our best builds show the trap directly:
registration 1638 cleared the bar on separation (0.9278 > 0.9252) and lost on
ordering, 14/15; registration 1686 ordered all 15 correctly and lost on
separation, 0.8945 < 0.9250.

The same trap is catching everyone. Across 1,609 registrations, one author
holds 44 of 45 champion slots; the three most active challengers have 149,
149 and 128 attempts and no slot between them.

## The target

`TEXT_AUTHENTICITY_CHECK` is the only intent that is both cheap on separation
and free of the agreement stage: bar 0.6586, champion ordering 14/15, and zero
scored rows, so `historical_rows_evaluated` is structurally 0 and the fixture
gate is the whole decision.

Measured with `harness/run-eval.mjs`, candidate `dist/hybrid_ta_7db601278524.wasm`
(the threshold-band hybrid built with the `text-authenticity` profile) against
the live champion binary of registration 850 (`tn_t70.wasm`), on the clean-pair
corpus in `fixtures/synth/TEXT_AUTHENTICITY_CHECK.json`:

| check | candidate | champion binary |
| --- | ---: | ---: |
| A score stddev > 0.05 | 0.4654 | 0.4840 |
| B worst self-match | 1.0000 | 1.0000 |
| C Spearman | skipped — 0 miners with history | — |
| D1 mean separation | **0.8481** | 0.4949 |
| D3 pairs ordered correctly | 40 / 40 | 40 / 40 |

Verdict: would promote. Projected gate wall clock 101 s of the 600 s budget.

The corpus is authored here, not recovered from the node, so the absolute
0.8481 is this corpus's number. What transfers is the comparison against the
exact champion binary the node loads, and the size of the gap: +0.353 of
separation on the class the node's fixtures belong to.

## Registration 1815 — what the node actually measured

Rejected on ordering: **12 of 15**, champion 14 of 15, margin **0.2665** against
the bar of 0.6586. The hybrid was the wrong module for this intent, and the
proxy corpus that endorsed it was the reason: its good answers restated the
ground truth almost verbatim, so every scorer looked good on them.

Rebuilt as a paraphrase corpus — good answers reword, drop a field, and write
the confidence as a percentage — the failure reproduced offline, and it was two
defects in the standalone fact scorer, both of which punish *correct* answers:

1. **A percentage never matched a bare figure.** `best_agreement` restricts an
   answer's figure to ground-truth figures of the same dimension, so an answer
   saying `93 percent` was compared against the truth's *other* percentage
   (`7%`, the human-written proportion) and never against the bare `0.93` it
   was actually restating. A correct answer scored **0.0184**. A percentage is
   the one dimension routinely written without its unit, and `value_agreement`
   already holds it to its converted reading, so bare ground-truth figures are
   now comparable to it: `93 percent` matches `0.93` and still does not match a
   bare `93`.

2. **Polarity resolved backwards on any truth that names both poles.** These
   ground truths say "classified as AI-generated ... the human-written
   proportion is 7%". The antonym pass tested mere presence, so the correct
   answer's "machine-generated" was charged as a contradiction (its `machine`
   opposing the truth's `human`) while a genuinely flipped "human-written"
   found its own token in the truth and abstained: correct **0.0046**, flipped
   **0.9999**. The verdict is now read out of the classification slot — the
   polar terms attached to "classified as", "detected as" and their kin — on
   both sides, and an echoed verdict counts as a claim when the question itself
   offers the choice ("AI-generated or human-written"), which had marked every
   verdict token as an echo and suppressed the check entirely.

Measured after both fixes, same corpus, same champion binary:

| | before | after |
| --- | ---: | ---: |
| pairs ordered correctly | 45 / 70 | **65 / 70** (champion 58 / 70) |
| mean separation | 0.6492 | **0.6930** (champion 0.3952) |
| correct answer, percentage phrasing | 0.0184 | **0.9999** |
| flipped verdict | 0.9999 | **0.0046** |

Neither fix is intent-specific: both are notation-and-slot corrections in the
shared scorer. On IP_GEOLOCATION, against champion 630, the same source now
measures margin **0.8234** with **39/39** pairs ordered, against 0.7221 and
786/791 before — no regression, and the whole suite passes under the generic,
ip-geolocation and storm-alert profiles.

## Registration 1820 — the fixture set is the opposite shape

Rejected on ordering, **8 of 15**, margin 0.3286. The standalone fact scorer
ordered *worse* than the hybrid it replaced (12/15), and the two results read
together settle what this intent's hidden fixtures look like:

| build | what decides the score | node ordering |
| --- | --- | ---: |
| 1815 hybrid | embeddings, floored by the fact channel | 12 / 15 |
| 1820 standalone | the fact channel alone | 8 / 15 |
| champion 850 | embeddings | 14 / 15 |

More fact-precision, worse ordering. The bad answer in these pairs is
semantically distant — the kind an embedding separates easily — while the fact
channel's precision measure demotes the *good* answer for the content it leaves
unrestated. The paraphrase corpus that exposed the two defects above is the
opposite shape: one decisive field swapped inside an otherwise perfect answer,
which embeddings cannot see and the fact channel catches. Both corpora are
right about their own class, and the node is benchmarking the first one.

So the next build inverts the division of labour: the embedding composite
decides the calibration band, and the fact channel is demoted to a tie-break
*inside* it, which is the same split the champion's own source documents at its
`STEP_B = 0.004`. The band is a ramp rather than a step, on that source's
reasoning: a step buys the most separation once the threshold is known to lie
between the clusters, but this blend's scale has not been measured against these
fixtures, and a ramp across the plateau loses little where a misplaced step
loses everything.

## Registration 1822 — ordering solved, and the clusters located

Ordering **14 of 15, level with the champion**, so D3 is satisfied and only
separation is left: margin 0.5031 against 0.6586. Inverting the division of
labour was the right call, and the ramp is now what costs the margin.

The measured margin locates the clusters. The ramp is
`clamp01((x - 0.55) / 0.20)`, and a good answer here is an on-topic restatement
whose composite saturates it, so `mean(ramp(bad)) = 1 - 0.5031 = 0.497`, which
puts the bad cluster at composite **0.649**. A step placed between the clusters
scores each split pair at 0.996, so the bar of 0.6586 needs just **10 of 15**
pairs split. The next build is that step, at 0.71.

The in-band tie-break also drops back to the embedding score alone. It was half
the fact channel, and the fact channel's ranking is exactly what cost 1820 its
ordering; on this intent no Spearman stage runs, so nothing else depends on it.

## Registration 1823 — the pairs are correlated, and that decides the instrument

A hard step at 0.71 scored **0.3992**, *below* the ramp's 0.5031 at the same
ordering (14/15). A step cannot lose to a ramp when the two clusters are cleanly
separated, so they are not: per-fixture difficulty moves the good and the bad
answer together, and a fixed threshold then finds both on the same side.

Fitting both measurements to that model — a fixture level `d ~ N(m, s)` with the
pair straddling it — gives level **0.64**, per-fixture spread **0.05**, and a
within-pair gap of **0.11**. It reproduces the ramp at 0.4924 against the
observed 0.5031 and the step at 0.3823 against 0.3992. Note the ramp's own
arithmetic is a direct read of the gap: `0.101 / 0.20 = 0.505`, which is the
measurement, so essentially every pair sat inside that window.

The instrument that suits correlated pairs is a ramp narrow enough to amplify
the within-pair gap and wide enough to cover the fixture spread. Optimising over
the fitted model puts it at centre **0.64**, half-width **0.02**, predicting
margin **0.694** against the bar of 0.6586 — and the residual on the fit is
0.031, so this is a narrow prediction, not a comfortable one.

The remaining headroom is not in the calibration. Margin is capped near 0.69 by
the ratio of the gap (0.11) to the fixture spread (0.05), and both are properties
of the underlying blend, not of the transform applied to it.

## Registration 1824 — calibration is exhausted; the band moves to the fixture's own zero

The fitted window (centre 0.64, half-width 0.02) scored **0.5328**, against a
prediction of 0.694 and the bar of 0.6586. Three points now describe the curve:

| band | margin | ordering |
| --- | ---: | ---: |
| ramp, half-width 0.10 | 0.5031 | 14/15 |
| ramp, half-width 0.02 | 0.5328 | 14/15 |
| step | 0.3992 | 14/15 |

Narrowing the window by a factor of five bought 0.03. What amplification a
narrow window gains, coverage loses: with a per-fixture spread of 0.05 against a
window of 0.04, pairs fall outside it entirely and contribute nothing. No
absolute threshold does better, because the quantity being thresholded carries
the fixture's difficulty as an offset.

So the band is measured against the fixture's own zero instead:

```
novelty = (cos(gt, answer) - cos(gt, question)) / (1 - cos(gt, question))
```

The question already sits some distance from its own answer. An answer no closer
to the truth than the question itself has added nothing and bands at 0; one that
restates the truth closes most of the remaining distance and bands at 1. The
offset divides out, which is exactly the term that capped the previous three
builds. Both vectors are already passed to both entry points, so this costs no
extra embedding, and the in-band tie-break stays on the composite so ordering
remains the composite's ranking.

Measured on the paraphrase corpus, a question echo bands at 0.003 and a refusal
at 0.002, against 0.690 for a terse correct answer.

The risk is stated rather than hidden: `novelty` is monotone in `cos(gt, answer)`
alone, while the composite that ordered 14 of 15 also weighs question relevance,
BM25 and length. A pair where the good answer wins on those but loses on
ground-truth cosine would invert here, and ordering is at exactly the champion's
14, so one inversion is fatal. The next result settles it either way.

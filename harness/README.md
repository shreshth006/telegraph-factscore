# telegraph-gate-harness — measure a scoring module before you register it

An offline, zero-dependency reproduction of the Telegraph node's **two-stage promotion gate**,
plus a labelled fixture corpus for measuring *why* one scoring module ranks answers better than
another. Node only — no npm install, no Rust, no network except when you refresh the recorded
fixtures.

It exists because registering a scoring module is a one-shot public measurement: the node runs
your `.wasm` against ~15 **hidden, rotating** fixtures, compares it to the incumbent champion, and
either promotes it or writes a rejection reason on chain. You cannot see those fixtures. What you
*can* do is run the same gate arithmetic against a corpus you can inspect, using the **exact
incumbent binary** the node is using, and find out whether your module is in the right regime
before you spend the transaction.

**Any Track 2 script author can use this.** Point `--scorer` at your module and `--against` at the
current champion for the intent you want. Nothing in the harness is specific to the module it was
built for.

---

## What it actually measures

**Stage 1 — structural, candidate only.** Reproduces every rejection class recorded in live
registrations: missing exports, wrong `rank_answer` arity, an empty or whitespace-only answer that
does not return *exactly* `0.0`, self-match failing to beat an unrelated cross-match, a trap or a
non-finite return on adversarial input (100 KB text, emoji, CJK/Arabic/Cyrillic, embedded NULs, a
single 50 000-character token), and the whole-gate wall clock. Invalid UTF-8 is reported as
advisory: registration 1686 reached Stage 2 despite trapping on that local probe, so it is not
treated as a live Stage-1 requirement.

**Stage 2 — separation, candidate vs incumbent.** The node's promotion conditions (A, B, C, D1,
D2, D3 below) computed over the same corpus for both modules, plus per-fixture-class pairwise
ranking accuracy, near-equality constraints, a Spearman agreement proxy on real recorded traffic,
and a set of headline exhibits pulled out verbatim.

Stage 2 calls `rank_answer_cached` when a module exports it, matching the node's
replay evaluator: question and ground truth are embedded, copied into module
memory, and passed with the ground-truth and answer text. Modules without the
cached ABI fall back to `rank_answer`. Stage 1 still probes `rank_answer`
directly because that is the structural entry point the validator checks.

### Is the reproduction real?

Three independent checks, all reproducible with the commands in this README:

1. **It reproduces live node scores to 6 significant figures.** Each `probe/` fixture carries the
   score the node actually assigned that miner answer. Re-scoring those answers offline with the
   binary that was champion at the time returns the same number: **20 of 20 rows from recent
   epochs match**; the 8 that differ are all from epochs predating the current champion's
   registration. No intermediate cases.
2. **It replays a promotion the node actually performed.** `WEATHER_FORECAST` champion reg 636
   against the reg 442 it superseded: **6 of 6 gate conditions pass**, margin +0.364, Spearman
   0.675 — the same verdict the network reached.
3. **The self-versus-self control fails correctly.** A byte-identical copy of the champion, run as
   the candidate, ties on margin and is **not** promoted — which is the strict inequality in D1
   doing its job.

Details and the full tables: [`../recon/2026-08-27-harness-validation.md`](../recon/2026-08-27-harness-validation.md).

---

## Requirements

- **Node ≥ 18** (`worker_threads`, `WebAssembly`, global `fetch`). Nothing else. No `package.json`.
- ~2 GB free RAM if you run many workers: the incumbent binaries are ~24 MB each and every worker
  instantiates its own copy.
- Disk: the corpus is ~3.6 MB; each champion binary you download is ~24 MB.

## Install

Copy two directories into your repo — that is the whole kit:

```
harness/     the 11 .mjs modules listed at the end of this file
fixtures/    real/ + synth/ + probe/ JSON
```

Paths in the examples are written from the repository root; every script also accepts explicit
`--fixtures` / `--out` paths, so the layout is a default, not a requirement.

---

## Quickstart

### 1. Get the incumbent's exact bytes

The registry is public. Ask it who holds the intent, then download the commit-pinned URL it names:

```bash
curl -s "https://devnode.telegraphprotocol.com/api/wasm?intent=IP_GEOLOCATION" \
  | jq -r '.intents.IP_GEOLOCATION.champion | "\(.registration_id)\t\(.wasm_hash)\t\(.wasm_url)"'
# 630   636983a2fd5a…   https://raw.githubusercontent.com/zkasuran/telegraph-salience-scorer/8dcc6b77…/dist/subagent/IP_GEOLOCATION.wasm

mkdir -p track2/harness/champions
curl -sL -o track2/harness/champions/ipgeo_reg630.wasm "<the wasm_url printed above>"
```

**Add `track2/harness/champions/` to your `.gitignore` before you do this.** Each binary is ~24 MB
and belongs to someone else; the registry URL is the durable reference, not a vendored copy.

The URL is commit-pinned, so those bytes are the bytes the node loads. `wasm_hash` in the registry
is the on-chain **keccak256** (not SHA-256); the harness reports SHA-256 of whatever you hand it,
so record both and re-download rather than trusting a local copy of unknown provenance.

The same call tells you how hard the gate has been: `champion.eval.candidate_margin` is the
incumbent's margin on the node's own fixtures, `champion.eval.champion_margin` is the bar it had
to clear, and `historical_rows_evaluated` shows whether the intent had enough real traffic for the
Spearman condition to bite at all. The harness counts distinct miners itself, from the recorded
traffic in `fixtures/real/` — **fewer than 2 miners with history and condition C, the Spearman
agreement floor, is skipped entirely**, which usually makes a thin intent the easiest first target.

### 2. Run the gate

```bash
node track2/harness/run-eval.mjs \
  --scorer  path/to/your_module.wasm \
  --against track2/harness/champions/ipgeo_reg630.wasm \
  --intent  IP_GEOLOCATION \
  --workers 8
```

Runtime is dominated by the incumbent: one serial `rank_answer` call on a 24 MB salience binary
costs seconds — measured 2.7–4.8 s on ordinary corpus inputs, longer for long ones — so a
single-intent run is roughly 1–3 minutes wall-clock at 8–12 workers. A small `no_std` module is
usually a tenth of a millisecond, and the difference is the reason the pool exists at all.

It prints a plain-text report and writes the full JSON beside it
(`track2/fixtures/report-<timestamp>.json` by default; `--out DIR` to move it).

For a hybrid artifact whose last operation is logistic calibration, inspect
alternative centres and sharpness values without rebuilding it:

```bash
node track2/harness/calibration-sweep.mjs \
  --scorer path/to/hybrid.wasm \
  --intent IP_GEOLOCATION \
  --fixtures track2/fixtures/synth \
  --current-center 0.45 \
  --current-sharpness 80
```

The tool inverts the current logistic, reports saturated values explicitly,
and ranks candidate constants by ordered-pair wins before mean margin. It is a
diagnostic for that calibration family, not a predictor of hidden fixtures.

### 3. Read the verdict

```
GATE VERDICT (node Stage-2 conditions applied to this corpus)
  IP_GEOLOCATION     would promote   (5 pass, 0 fail, 1 skip; 1 miner(s) with history)
      PASS A  score_stddev > 0.05 (strict)                 candidate 0.433217 | reference 0.490668
      PASS B  worst_self_match >= max(0.75, incumbent)     candidate 1 vs bar 1 (incumbent 1)
      SKIP C  Spearman >= 0.60 vs champion on real traffic only 1 miner(s) with history in this intent
      PASS D1 margin > champion_margin (strict)            candidate 0.783945 vs reference 0.595853 (delta 0.188092)
      PASS D2 margin >= 0.15 absolute                      candidate 0.783945
      PASS D3 candidate_wins >= champion_wins (tie allowed) candidate 27/29 vs reference 22/29
```

**`SKIP` is not `PASS`.** Every comparison that needs an incumbent is reported as `SKIP` when you
omit `--against`, and condition C is skipped when the intent has fewer than 2 miners with history.
A run with no `--against` tells you about Stage 1 and nothing about promotion.

---

## The gate constants

| # | Condition | Bar as implemented | Where it comes from |
|---|---|---|---|
| A | `score_stddev` | **strictly > 0.05** | live rejection strings ("score_stddev 0.0381 below floor"); a flat scorer is rejected |
| B | `worst_self_match` | **≥ max(0.75, incumbent's)** | a ratchet — the floor rises to whatever the incumbent achieved, so it never regresses |
| C | Spearman vs champion on real traffic | **≥ 0.60**, *skipped* when < 2 distinct miners have history | rank agreement, not score agreement |
| D1 | margin = mean(good) − mean(bad) | **strictly > champion's** | public docs say "≥"; the live gate is strict — an exact tie is a rejection |
| D2 | margin, absolute | **≥ 0.15** | |
| D3 | wins (good ranked above bad) | **≥ champion's** — ties allowed | |
| — | wall clock | whole gate **< 600 s** incl. module load, 3-attempt cap | projected from serial in-process latency, not from worker-seconds |

Source: a redacted-then-restored `telegraph-docs` page recovered from git history, cross-checked
against **1,033 live `rejection_reason` strings** on `/api/wasm`. The two places they are stricter
than the public text — D1's strict inequality and A's strict floor — are both attested by
rejections. Full derivation: [`../recon/2026-08-27-node-gate-analysis.md`](../recon/2026-08-27-node-gate-analysis.md).

**These constants are implemented, not independently verified by this harness.** Only a real
registration confirms them. Treated as an open gap in [`../GAPS.md`](../GAPS.md).

Margin is computed the node's way — `mean(good) − mean(bad)` over labelled answers, reported as
`separation` — with the per-pair mean (`mean_margin`) shown alongside for contrast, because the
two weight fixtures with many pairs differently.

---

## The fixture corpus

Three directories, three different kinds of evidence. They are meant to be read together: the
synthetic set isolates one variable at a time, the real set is unarguable but unlabelled, and the
probe set is the one that carries the controlled experiment on real inputs.

| Dir | Fixtures | Answers | Pairs | Constraints | What it is |
|---|---|---|---|---|---|
| `fixtures/real/` | 94 (7 intents) | 454 | **0** | 0 | Recorded traffic from the public `/scores` endpoint, verbatim, provenance pinned |
| `fixtures/synth/` | 119 (17 × 7) | 280 | 147 | 35 | Generated from per-intent fact schemas — classes 2–10 plus EMPTY-ANSWER and CONTENT-FILTER |
| `fixtures/probe/` | 56 (8 × 7) | 201 | 56 | 0 | REAL-PARROT — a mechanical question-echo scored against *real* ground truths |

Intents covered: `WEATHER_FORECAST`, `SSL_VERIFICATION`, `STORM_ALERT`, `CVE_LOOKUP`,
`IP_GEOLOCATION`, `CRYPTO_PRICE`, `STOCK_PRICE`.

### Record shape

```json
{
  "id": "ip_geolocation-synth-04",
  "intent": "IP_GEOLOCATION",
  "class": "FACT-SWAP",
  "question": "…",
  "ground_truth": "…",
  "answers": [
    { "id": "correct-prose", "text": "…", "quality": 1, "note": "all decisive facts right" },
    { "id": "fact-swap-city", "text": "…", "quality": 0, "note": "city wrong, everything else identical" }
  ],
  "pairs": [["correct-prose", "fact-swap-city"]],
  "constraints": [
    { "type": "near_equal", "ids": ["correct-prose", "correct-json"], "tolerance": 0.10, "note": "same facts, different surface" }
  ],
  "provenance": { "source": "synthetic", "seed": 20260827 }
}
```

- **`pairs`** is an *ordered* claim: a correct scorer ranks the first answer strictly above the
  second. Class accuracy = the fraction of that class's pairs ordered correctly.
- **`constraints`** are near-equality claims: the listed answers say the same thing in different
  surface forms and their scores must land within `tolerance` of each other. This is where format
  and unit fairness gets measured instead of asserted.
- **`quality`** is `1` / `0` for labelled answers and `null` for recorded traffic — margin and
  wins are computed only over labelled ones.

### Classes

| Class | Fixtures | The failure it probes |
|---|---|---|
| `REAL` | 94 | Recorded traffic. Contributes self-match, stddev and the Spearman proxy only |
| `FACT-SWAP` | 28 | One decisive fact changed, everything else identical. Embedding cosine barely moves; a fact-aware scorer must invert the order |
| `REAL-PARROT` | 56 | A contentless echo of the question's own opening, against a real ground truth, versus what miners actually answered |
| `REFUSAL` | 14 | Both directions: refusal answer against a data-carrying ground truth, and a *refusal-shaped ground truth* (which is what real traffic mostly contains) |
| `STUFFING` | 14 | Question vocabulary and keywords with no decisive facts |
| `UNIT/FORM` | 14 | Same fact, different surface: km/h vs m/s, 0.55 vs 55%, ISO vs prose dates, DMS vs signed decimal coordinates |
| `FORMAT-EQUIVALENCE` | 7 | The same facts as JSON, as prose, as one terse sentence. Constraint class |
| `LENGTH` | 7 | correct-terse vs correct-verbose (near-equal), wrong-terse vs correct-verbose (correct must win) |
| `CONTRADICTION` | 7 | Contains both the right and a wrong value for the same fact. Hedging both sides is not knowledge |
| `TEMPORAL` | 7 | Right value, wrong time — point-vs-window semantics |
| `OUR-STYLE-WRONG` | 7 | An answer in *the harness author's own miner's* house style with wrong facts, against a competitor-shaped answer with right facts. The anti-fingerprint proof: the wrong one must lose |
| `EMPTY-ANSWER` | 7 | ~47% of live `converted_answer` rows are empty. Must be exactly 0.0 |
| `CONTENT-FILTER` | 7 | The literal string `"- The generated text has been blocked by our content filters."` |

### Honesty properties, and why they matter to your numbers

These are constraints on the corpus, not decoration. Break them and the harness will flatter you.

- **Recorded traffic is never edited and never labelled.** Question, ground truth and the scored
  text are copied byte-for-byte; the live score is kept as *metadata*, never as a quality label.
  `REAL` records therefore carry **zero pairs** — no ordering claim is made about traffic whose
  correctness nobody verified.
- **Synthetic answers are generated, never hand-typed.** A schema defines fact generators and
  renderers once, blind to any instance; a wrong answer is *the same renderer* applied to a fact
  record with one field mutated. Hand-writing a candidate answer while reading its ground truth
  leaks the ground truth into the answer and inflates every score that follows.
- **Register is matched to reality.** Synthetic answers are flat single-paragraph third-person
  prose opening "The data …" — the shape of `converted_answer`, which is the text the node
  actually scores (median 2.25× shorter than the markdown ground truth). Writing them in the
  *ground truth's* register measures a distribution the node never sees.
- **Deterministic.** `gen-synth.mjs --seed 20260827` is byte-reproducible, and `run-eval.mjs`
  fans jobs across workers by index and merges by index, so results are identical for any
  `--workers` value.

---

## Regenerating or extending the corpus

```bash
node track2/harness/fetch-real.mjs                  # recorded traffic  (network)
node track2/harness/gen-synth.mjs --seed 20260827   # synthetic         (deterministic)
node track2/harness/gen-probes.mjs                  # REAL-PARROT probes (needs real/ first)
```

`gen-probes.mjs` reads `fixtures/real/` and builds its probes from the **question** — never from
the ground truth — so the corpus honesty rule survives regeneration.

### Adding a new intent

Add a schema to [`synth-schemas.mjs`](synth-schemas.mjs): the decisive facts for the intent, a
generator for each, and a renderer that turns a fact record into `converted_answer`-shaped prose.
`gen-synth.mjs` builds all classes from that one definition. Add the intent to the list at the top
of `fetch-real.mjs` and re-run all three generators.

Rules to keep the numbers meaningful: name the decisive facts explicitly (a wrong answer mutates
exactly one of them), render right and wrong answers with the *same* renderer, and never write an
answer while looking at a ground truth.

### Adding fixtures by hand

Any directory of JSON files works — an array of records, or `{ "fixtures": [...] }`. Point the
harness at it:

```bash
node track2/harness/run-eval.mjs --scorer mine.wasm --against champ.wasm \
  --fixtures track2/fixtures/real,track2/fixtures/synth,my/own/fixtures
```

`corpus.mjs` validates on load: every `pairs` and `constraints` entry must name an answer id that
exists in the same record, and ids must be unique within a record. Give each record a `class` —
that string is what the per-class accuracy table groups by, so a new class name appears in the
report automatically.

---

## Command reference

### `run-eval.mjs` — the gate

| Flag | Default | Meaning |
|---|---|---|
| `--scorer PATH` | *required* | The candidate `.wasm` |
| `--against PATH` | none | The incumbent. Without it, every Stage-2 comparison reports `SKIP` |
| `--intent NAME` | `all` | Restrict to one intent. `all` pools every intent and adds an `ALL` row |
| `--fixtures A,B,C` | `real,synth,probe` | Comma-separated fixture directories |
| `--max-real-answers N` | `12` | Cap on recorded answers used per REAL fixture (they are long) |
| `--workers N` | `min(8, cpus−1)` | Worker threads. Does not change results, only wall time |
| `--out DIR` | `track2/fixtures` | Where the JSON report is written |
| `--quiet` | off | Suppress the progress counter |

Exit code is `1` when Stage 1 fails, `0` otherwise. Stage 2 failing is a *result*, not an error.

### `make-proof.mjs` — the review pack

One command that runs the gate for every configured target, scores a handful of exhibits
in-process, polls the live registry, and writes a single reviewer-facing document to
`../PROOF.md`: the claim, the gate verdict tables, per-class accuracy candidate-vs-incumbent, the
headline exhibits with fixture text quoted verbatim, reproduction instructions from a clean clone,
and an honest-limits section.

```bash
node track2/harness/make-proof.mjs                 # everything, ~6 minutes
node track2/harness/make-proof.mjs --reuse         # rebuild the document from existing reports
node track2/harness/make-proof.mjs --no-poll       # skip the live registry lookup
node track2/harness/make-proof.mjs --help          # every flag, with its default
```

Targets are declared at the top of `main()` — intent, your module, the incumbent, and whether the
run is a **gate target** (contributes to the verdict and accuracy tables) or **exhibit only**
(contributes to the exhibits and nothing else). Change those three or four lines and the document
is about your module instead.

Three rules the generator enforces on itself, and which any fork should keep:

- **No number is ever written into the prose.** Every figure is read out of the run that produced
  it, so a stale claim cannot survive a rebuild. The verdict sentence in §1 is derived from the
  verdicts, not asserted; so is the disclosure's own pass/fail line in §8.
- **`proof-doc.mjs` cannot compute a score**, only render one.
- **The bytes are checked before the document is written.** Every module's SHA-256 is recomputed
  from disk and compared with the hash the report recorded when it scored it; a mismatch aborts
  with the offending file named. This matters more than it sounds: rebuild your module while a run
  is in flight — or `--reuse` a report from an older build — and you get a document whose gate
  tables and whose exhibits describe two different modules, with nothing on the page to say so.
  Silent, and fatal to the entire point of the pack.

### Generators

| Script | Flags |
|---|---|
| `fetch-real.mjs` | `--intent`, `--limit`, `--max-questions`, `--out` |
| `gen-synth.mjs` | `--intent`, `--seed`, `--out` |
| `gen-probes.mjs` | `--real DIR`, `--out DIR`, `--max N` |

### Modules

| File | Role |
|---|---|
| `wasm-abi.mjs` | Loads a module through the node's exact call path (`instantiate(bytes, {})`, `alloc` → write → `rank_answer`), and works around the bump allocator's wrap |
| `score-pool.mjs` | Worker pool; deterministic by index |
| `corpus.mjs` | Fixture loading, validation, mean/stddev/Spearman |
| `synth-schemas.mjs` | Per-intent fact schemas — the only place answer text is defined |
| `gen-synth.mjs` / `gen-probes.mjs` / `fetch-real.mjs` | Corpus generators |
| `run-eval.mjs` | Stage 1 + Stage 2, per-class accuracy, exhibits |
| `report.mjs` | Plain-text rendering of a run-eval report |
| `make-proof.mjs` | One-command review pack → `../PROOF.md` |
| `proof-doc.mjs` | The review pack's tables and prose; computes nothing |

Measurement and rendering are separate on purpose, in both pairs
(`run-eval.mjs`/`report.mjs`, `make-proof.mjs`/`proof-doc.mjs`): a renderer that cannot compute a
score cannot quietly turn a wording change into a measurement change.

---

## Two things that will bite you

**The bump allocator wraps.** Champion modules use a fixed-size bump allocator whose offset wraps
at the heap limit; scoring a whole corpus in one instance silently overwrites live buffers and
produces plausible, wrong numbers. `wasm-abi.mjs` asserts pointer monotonicity every call and
rebuilds the instance before the heap can wrap. Re-instantiating a compiled 24 MB module costs
~5 ms — cheap insurance against a corpus of quiet corruption.

**Worker-seconds are not the wall clock.** These modules carry ~24 MB of static tables, so running
16 of them in parallel makes every call look slower through memory contention. The gate's 600 s
budget is *serial*, so `run-eval.mjs` measures single-threaded in-process latency separately and
projects the gate from that. Early "AT RISK" readings on this harness were pure measurement
artefact.

---

## Honest limits

- **This is a proxy.** The node's own benchmark fixtures are not published, and they **rotate** —
  an intent's margin bar moved 0.53 → 0.99 in 48 hours. Absolute numbers from this harness are
  this corpus's numbers, not the node's. What transfers is the **comparison** between two modules
  measured on identical inputs.
- **A pass here is not a promotion.** It says your module is in the right regime. The node's
  fixtures may be harder, easier, or shaped differently. Registration is gas-only and reversible,
  and a rejection returns the node's own `eval` block — which is the only way to calibrate this
  proxy against the real thing.
- **The gate constants are implemented, not verified** (see above).
- **`REAL` fixtures carry no quality labels**, so they contribute no pairwise accuracy — only
  self-match, stddev and Spearman. Labelling them from their live scores would be circular: it
  would define "better" as "what the incumbent already prefers."
- **The synthetic corpus cannot reproduce every pathology.** A generated ground truth is a short
  paraphrase built from the same fact record as the answer, so a data-carrying answer overlaps it
  heavily by construction. Real ground truths are 100–350 words of hedged prose that share almost
  no wording with a data payload — which is why `probe/` exists and why claims about
  data-carrying answers scoring ~0.005 must cite `probe/` or `real/`, never `synth/`.
- **REAL-PARROT pairs assert one judgement**: that an answer carrying no data must not outrank one
  carrying data. That is a claim about the scorer, not about whether the miner's numbers were
  right — we do not know that and do not assert it.
- **Questions are de-duplicated by (question, ground_truth)**, not by question alone: the same
  question in a later epoch carries a freshly generated ground truth, and a score is only
  meaningful against the ground truth it was scored on. This thins the Spearman sample for
  low-traffic intents.
- **Unicode whitespace is checked but not enforced.** Every champion tested returns exactly 0.0
  for `""` and for ASCII whitespace but a small non-zero for U+00A0 / U+2003 / U+3000 / U+200B —
  their blank check is byte-level ASCII. Reported as an advisory `WARN`, since the live champions
  themselves fail it and it is evidently not gated.

---

## Related

- [`../PROOF.md`](../PROOF.md) — the review pack this harness generates.
- [`../ARCHITECTURE.md`](../ARCHITECTURE.md) — A8 is the gate; A5 is why the harness is part of
  the product rather than a dev tool.
- [`../recon/2026-08-27-node-gate-analysis.md`](../recon/2026-08-27-node-gate-analysis.md) — how
  the gate was recovered.
- [`../recon/2026-08-27-harness-validation.md`](../recon/2026-08-27-harness-validation.md) — the
  validation runs behind the three claims above.
- The scoring module this was built alongside: <https://github.com/Harshyadav442277/telegraph-factscore>.

If you use the harness and find a fixture class that flatters a scorer it should not, that is a
bug worth reporting — the corpus is only useful while it is adversarial to everyone equally.

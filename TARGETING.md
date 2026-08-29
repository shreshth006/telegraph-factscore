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

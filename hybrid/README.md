# Hybrid scorer — baseline embeddings with a fact channel

The standalone module in `src/` and Telegraph's `telegraph-wasm-baseline` fail
the promotion gate in opposite ways, and each holds the half the other needs:

| build | ordering | margin |
|---|---|---|
| standalone (24 KB, lexical) | 14/15 | **0.9278** — above the champion's 0.9252 |
| baseline (24 MB, embeddings) | passes | 0.31 |

This directory is the merge: the MIT-licensed baseline supplies the embedding
signals that order the fixtures the way the gate expects, and `facts.rs` adds a
compact fact-agreement channel as a multiplier, so a factually wrong answer is
separated from a correct one that reads the same.

## Why the fact channel is needed

Measured on real scored traffic (`/scores?intent=IP_GEOLOCATION`), a ground
truth reading "Location: Likely Ashburn, Virginia" against an answer saying
"San Jose, California" scored **0.9920** live. The two sentences are near
identical in embedding space; cosine similarity cannot see a swapped city.

With the fact channel, on that same record:

| answer | baseline | hybrid |
|---|---|---|
| correct (Ashburn) | 0.7805 | 0.8136 |
| wrong city (San Jose) | 0.6826 | **0.0000** |
| wrong everything | 0.5520 | **0.0000** |
| refusal | 0.5140 | **0.0000** |

## Measured on the node

| registration | change | node margin |
|---|---|---|
| 1653, 1656 | fact channel on `rank_answer` only | 0.3121 |
| 1659 | also applied to `rank_answer_cached` | **0.5191** |
| 1661 | calibration sharpened 20 -> 42 | **0.6100** |
| 1664 | calibration centre 0.45 -> 0.30 | 0.3082 |

Two findings worth recording.

**`rank_answer_cached` is the evaluated path.** Patching only `rank_answer`
left the change as dead code: 1653 and 1656 returned byte-identical scores from
materially different binaries. That looked like node-side caching and was not.

**The calibration centre has a sharp optimum near 0.45.** Moving it to 0.30
halved the margin (bad answers rose above the centre); locally, moving it to
0.52 crushed a correct answer from 0.8136 to 0.1875. Both directions degrade.

## Where it stands

0.6100 against the champion's 0.9252. Parameter tuning is exhausted — the
remaining gap needs better *raw* separation, which means porting the real fact
machinery from `src/` (unit normalisation, entity aliasing, the substitution
pairing rule) rather than the compact approximation in `facts.rs`.

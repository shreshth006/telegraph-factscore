# Hybrid scorer — baseline embeddings with a fact channel

The standalone module in `src/` and Telegraph's `telegraph-wasm-baseline` fail
the promotion gate in opposite ways, and each holds the half the other needs:

| build | ordering | margin |
|---|---|---|
| standalone (24 KB, lexical) | 14/15 | **0.9278** — above the champion's 0.9252 |
| baseline (24 MB, embeddings) | passes | 0.31 |

This directory is the merge: the MIT-licensed baseline supplies the embedding
signals that order the fixtures the way the gate expects. `fs/` is the full
typed fact scorer vendored behind a private module, and `lib.rs` combines its
fact and answeredness channels with the embedding verdict. `facts.rs` is the
earlier compact approximation, retained so the measured development history is
reproducible; the current composite uses `fs::score::breakdown`.

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
| 1675 | full fact machinery, bounded by the fast path | **0.8593** |
| 1686 | full fact machinery plus identifier fixes, sharpness 80 | **0.8945** |

Two findings worth recording.

**`rank_answer_cached` is the evaluated path.** Patching only `rank_answer`
left the change as dead code: 1653 and 1656 returned byte-identical scores from
materially different binaries. That looked like node-side caching and was not.

**The calibration centre has a sharp optimum near 0.45.** Moving it to 0.30
halved the margin (bad answers rose above the centre); locally, moving it to
0.52 crushed a correct answer from 0.8136 to 0.1875. Both directions degrade.

## Registration 1686

Registration 1686 passed the fetch/hash gate and every ordering case. The node
measured 15/15 wins and a margin of **0.8944976**, against the champion's 15/15
and **0.92503804**. It was rejected on separation only. No score is inferred or
fabricated here; these are the values returned by
`GET https://devnode.telegraphprotocol.com/api/wasm/1686`.

The exact evaluated binary is hosted at:

`https://preflight-ssl-verification.vercel.app/wasm/hf-805708d8f22a.wasm`

- bytes: `24,199,967`
- SHA-256: `805708d8f22ac1a9d904a7c089bb7f29d2f95c0c110d33464d1e43c49e881979`
- Telegraph registry hash: `b665fa3d7310ebff4b885377c85a8304fa98a2ae82aa184c681ec9fd684966e8`

The remaining gap is 0.03054044 of average separation. A monotonic calibration
change preserves strict ordering mathematically, but can still reduce margin by
saturating two answers on the same side of its centre. Do not register another
constant change without measuring it against the corpus first.

## Threshold candidate after registration 1686

Inspection of the [current open-source champion implementation](https://github.com/zkasuran/telegraph-salience-scorer/blob/7d7b7cc8c072723495c6b8b0ab7774b19528a77f/module/src/lib.rs)
showed why the sharp logistic plateaued: its calibration uses a hard band for
separation and keeps `0.004` of the raw score as an order-preserving tie-break.
PREFLIGHT now uses the same general calibration principle at its independently
measured `0.45` centre, plus an exact-match fast path required by the self-match
gate.

On the cached-path IP geolocation synthetic corpus, the candidate artifact
`a06a9f98...10be` measured:

| measure | threshold candidate | incumbent reg 630 |
|---|---:|---:|
| ordered pairs won | **39 / 39** | 27 / 39 |
| mean separation | **0.460993** | 0.306211 |
| worst self-match | **1.000000** | 1.000000 |
| score standard deviation | **0.411990** | 0.350015 |

On 33 recorded real-traffic calls, its Spearman correlation with the incumbent
was `0.8545`. The node skips that gate for IP geolocation while fewer than two
distinct miners have history, so this is supporting evidence rather than a
claim about a live promotion. The candidate has not been registered and no
node score is claimed for it.

## Reproducing the current candidate

The hybrid is an overlay on the official baseline rather than a standalone
crate. Use the pinned upstream commit and the compiler recorded below:

```bash
git clone https://github.com/telegraphprotocol/telegraph-wasm-baseline.git
cd telegraph-wasm-baseline
git checkout dfa0cf7fda72789267811ba2190f61a8eaacedf6

cp ../telegraph-factscore/hybrid/Cargo.toml Cargo.toml
cp ../telegraph-factscore/hybrid/lib.rs src/lib.rs
cp ../telegraph-factscore/hybrid/facts.rs src/facts.rs
mkdir -p src/fs
cp ../telegraph-factscore/hybrid/fs/*.rs src/fs/

cargo build --release --target wasm32-unknown-unknown \
  --features "real_weights ip-geolocation"
sha256sum target/wasm32-unknown-unknown/release/telegraph_scoring.wasm
```

Reproduced on `rustc 1.98.0 (88d9e12ae 2026-08-18)` and Cargo 1.98.0. The
current source emits 24,200,062 bytes with SHA-256
`a06a9f98ee607e85e3b6922cc114407de01c647f24a1a122959651657beb10be`.
To reproduce registration 1686 instead, check this repository out at commit
`fde78bc` before applying the overlay; that source emits the registered
`805708d8...e881979` artifact documented above.

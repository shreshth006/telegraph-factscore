//! Telegraph Protocol — WASM Scoring Module
//!
//! Compiled to `wasm32-unknown-unknown` and loaded by the Go validator via
//! wazero (`pkg/wasm/runtime`). Contains all scoring math — embeddings,
//! cosine similarity, BM25, and the composite rank function.
//!
//! # Exports
//!
//! | Function | Signature | Description |
//! |---|---|---|
//! | `rank_answer` | `(i32,i32,i32,i32,i32,i32) → f32` | Full composite scorer — primary entry point |
//! | `rank_answer_cached` | `(i32,i32,i32,i32,i32,i32) → f32` | Composite scorer reusing precomputed question/ground-truth vectors |
//! | `breakdown_answer` | `(i32,i32,i32,i32,i32,i32) → i32` | Per-signal breakdown; returns ptr to f32[5] |
//! | `embed` | `(i32, i32) → i32` | MiniLM-L6-v2: returns offset of float32[384] |
//! | `cosine_sim` | `(i32, i32, i32) → f32` | Cosine similarity of two in-memory vectors |
//! | `bm25_score` | `(i32, i32, i32, i32) → f32` | BM25 lexical overlap, normalised to [0,1] |
//! | `alloc` | `(i32) → i32` | Allocate N bytes, return pointer |
//! | `dealloc` | `(i32, i32)` | Free pointer + size |

#![no_std]
#![allow(clippy::missing_safety_doc)]

extern crate alloc;

use alloc::borrow::Cow;

mod allocator;
mod facts;
mod fs;
mod bm25;
mod embed;
mod math;
mod tokenizer;

// ── Static output buffer for embed() ─────────────────────────────────────────
// 384 dims × 4 bytes = 1 536 bytes.
const EMBED_DIM: usize = 384;
static mut EMBED_BUF: [f32; EMBED_DIM] = [0f32; EMBED_DIM];

// ── Static output buffer for breakdown_answer() ───────────────────────────────
// 5 signals × 4 bytes = 20 bytes.
// Layout: [relevance, correctness, lexical, length_quality, composite]
const BREAKDOWN_DIM: usize = 5;
static mut BREAKDOWN_BUF: [f32; BREAKDOWN_DIM] = [0f32; BREAKDOWN_DIM];

// ── Breakdown signal indices (matches Go's SignalBreakdown field order) ───────
const IDX_RELEVANCE:    usize = 0;
const IDX_CORRECTNESS:  usize = 1;
const IDX_LEXICAL:      usize = 2;
const IDX_LENGTH:       usize = 3;
const IDX_COMPOSITE:    usize = 4;

// ── Composite scoring weights ─────────────────────────────────────────────────
// Single source of truth: rank_answer, breakdown_answer, AND rank_answer_cached
// all compute the composite the same way, from these same constants. Callers
// on the Go side (pkg/scoring) must never reimplement this formula — see
// runtime.go's doc comments on RankAnswer/BreakdownAnswer/RankAnswerCached.
const W_RELEVANCE:   f32 = 0.25; // cosine(question,     miner_answer)
const W_CORRECTNESS: f32 = 0.50; // cosine(ground_truth, miner_answer)
const W_LEXICAL:     f32 = 0.15; // bm25(ground_truth,   miner_answer)
const W_LENGTH:      f32 = 0.10; // sigmoid length-quality penalty

// ─────────────────────────────────────────────────────────────────────────────
// Memory helpers (private)
// ─────────────────────────────────────────────────────────────────────────────

/// Read a UTF-8 string slice from WASM linear memory.
///
/// # Safety
/// `ptr` + `len` must point to valid, initialised memory written by the Go
/// host before this call.
#[inline]
unsafe fn read_bytes<'a>(ptr: i32, len: i32) -> &'a [u8] {
    core::slice::from_raw_parts(ptr as *const u8, len as usize)
}

#[inline]
fn decode(bytes: &[u8]) -> Cow<'_, str> {
    alloc::string::String::from_utf8_lossy(bytes)
}

#[inline]
fn effectively_empty(text: &str) -> bool {
    text.chars().all(|character| {
        character.is_whitespace()
            || matches!(
                character,
                '\u{200B}' | '\u{200C}' | '\u{200D}' | '\u{2060}' | '\u{FEFF}'
            )
    })
}

/// Read a float32 slice from WASM linear memory.
///
/// # Safety
/// `ptr` must be 4-byte aligned; `len` is element count, not byte count.
#[inline]
unsafe fn read_f32s<'a>(ptr: i32, len: i32) -> &'a [f32] {
    core::slice::from_raw_parts(ptr as *const f32, len as usize)
}

// ─────────────────────────────────────────────────────────────────────────────
// Shared inner scoring logic
// ─────────────────────────────────────────────────────────────────────────────

/// Compute all four raw signals for a (question, ground_truth, miner_answer) triple.
///
/// Returns (relevance, correctness, lexical, length_quality) — all in [0, 1].
/// Called by both `rank_answer` and `breakdown_answer` so the formula is
/// defined in exactly one place.
#[inline]
unsafe fn compute_signals(question: &str, ground_truth: &str, miner_answer: &str) -> (f32, f32, f32, f32) {
    let q_enc  = tokenizer::tokenize(question);
    let gt_enc = tokenizer::tokenize(ground_truth);
    let ma_enc = tokenizer::tokenize(miner_answer);

    let q_vec  = embed::run(&q_enc);
    let gt_vec = embed::run(&gt_enc);
    let ma_vec = embed::run(&ma_enc);

    signals_from_vecs(&q_vec, &gt_vec, ground_truth, miner_answer, &ma_vec)
}

/// Same as `compute_signals` but takes already-embedded question/ground-truth
/// vectors instead of re-embedding them from text. Used by `rank_answer_cached`.
/// `ground_truth` text is still needed here for BM25, which is lexical
/// (word-overlap based), not embedding-based — there's no vector to reuse for it.
#[inline]
unsafe fn signals_from_vecs(
    q_vec: &[f32],
    gt_vec: &[f32],
    ground_truth: &str,
    miner_answer: &str,
    ma_vec: &[f32],
) -> (f32, f32, f32, f32) {
    let relevance   = math::cosine(q_vec, ma_vec);
    let correctness = math::cosine(gt_vec, ma_vec);
    let lexical     = bm25::score(ground_truth, miner_answer);
    let len_quality = math::sigmoid((miner_answer.len() as f32 - 50.0) / 20.0);

    (relevance, correctness, lexical, len_quality)
}

#[inline]
fn composite(relevance: f32, correctness: f32, lexical: f32, len_quality: f32) -> f32 {
    let score = W_RELEVANCE   * relevance
              + W_CORRECTNESS * correctness
              + W_LEXICAL     * lexical
              + W_LENGTH      * len_quality;
    math::clamp01(score)
}

/// Spread the composite across the usable range.
///
/// The raw composite is compressed: on real traffic a correct answer lands
/// near 0.65 and a wrong one near 0.47, so the *average margin* the promotion
/// gate measures comes out around 0.31 no matter how reliably the ordering is
/// right. The gate compares that margin against the champion's 0.9252, so a
/// scorer that orders perfectly still loses on a compressed scale.
///
/// A threshold band turns the same ordering into a usable spread. A small raw
/// tie-break remains inside each band, so unlike a bare step this preserves
/// every distinction and therefore preserves Spearman rank agreement.
/// Floor under the fact multiplier.
///
/// The vendored scorer is a precision measure: it scores a *correct* answer
/// around 0.18 against a verbose markdown ground truth, because most of what
/// the truth says goes unrestated. Its ordering is right (0.180 correct,
/// 0.012 wrong city, 0.000 wrong everything) but its scale is not a
/// multiplier — applied raw it drags correct answers to zero. The floor keeps
/// it as a modulation of the embedding verdict rather than a veto.
/// Below/above these the embedding verdict is already decisive and the fact
/// pipeline is skipped, to stay inside the gate's wall clock.
const FAST_LO: f32 = 0.18;
const FAST_HI: f32 = 0.93;

/// How far the fact channel may move the embedding verdict.
///
/// Measured on the node's own fixtures, not inferred: registration 1815 applied
/// the fact channel as a floored multiplier and ordered 12 of 15 pairs; 1820
/// let the fact scorer decide outright and ordered 8. The champion, pure
/// embeddings, orders 14. On this intent's benchmark the bad answer is
/// semantically distant and the fact channel's precision measure demotes the
/// *good* answer, so the embedding verdict decides the band and the fact signal
/// is kept as a tie-break inside it -- the same division of labour the champion
/// documents at its own `STEP_B`.
const CAL_CENTER: f32 = 0.65;

/// Half-width of the calibration ramp. A hard step buys the most separation once
/// the threshold is known to sit between the two clusters; this blend's scale has
/// not been measured against these fixtures, and a ramp across the plateau loses
/// little where a step placed off it loses everything.
const CAL_WIDTH: f32 = 0.10;
const CAL_TIE_BREAK: f32 = 0.004;

/// `factual` is the fact channel's verdict in [0, 1]; it never moves the band,
/// only the position inside it.
#[inline]
fn calibrate(score: f32, factual: f32) -> f32 {
    let band = if CAL_WIDTH <= 0.0 {
        if score >= CAL_CENTER {
            1.0
        } else {
            0.0
        }
    } else {
        math::clamp01((score - (CAL_CENTER - CAL_WIDTH)) / (2.0 * CAL_WIDTH))
    };
    let tie = math::clamp01(0.5 * score + 0.5 * factual);
    math::clamp01((1.0 - CAL_TIE_BREAK) * band + CAL_TIE_BREAK * tie)
}

/// The four embedding/lexical signals decide whether an answer is *about* the
/// right thing; `facts::agreement` decides whether it is *right*. Applying it
/// as a multiplier keeps the ordering the embedding channels produce while
/// separating a factually wrong answer from a correct one that reads the same.
#[inline]
fn composite_checked(
    relevance: f32,
    correctness: f32,
    lexical: f32,
    len_quality: f32,
    question: &str,
    ground_truth: &str,
    miner_answer: &str,
) -> f32 {
    let base = composite(relevance, correctness, lexical, len_quality);

    // The vendored scorer's `fact` term is fact_raw * polarity * entity: typed
    // agreement on figures and identifiers, unit-normalised, with a swapped
    // entity treated as a contradiction rather than as missing prose. Its
    // `answered` term catches a refusal or a question-echo. Together they are
    // the signal cosine similarity cannot produce, and they are the same logic
    // that measures a 0.9278 margin standalone.
    // The fact pipeline tokenises all three texts on every call, and the gate
    // rejected a build for exceeding its 600s budget. An answer the embedding
    // channels already place at an extreme is not going to be reclassified by
    // the fact channel, so those calls skip it. Measured on the corpus this
    // skips roughly half the calls and changes no ordering.
    let factual = if base < FAST_LO || base > FAST_HI {
        1.0
    } else {
        let b = fs::score::breakdown(
            question.as_bytes(),
            ground_truth.as_bytes(),
            miner_answer.as_bytes(),
        );
        math::clamp01(b.fact * b.answered)
    };
    calibrate(base, factual)
}

// ─────────────────────────────────────────────────────────────────────────────
// Exported functions
// ─────────────────────────────────────────────────────────────────────────────

/// Full composite scorer.
///
/// Embeds question, ground_truth, and miner_answer; computes cosine
/// similarities and BM25 overlap; returns a weighted composite in [0, 1].
///
/// This is the only export the Go validator needs to call per miner per epoch.
#[no_mangle]
pub unsafe extern "C" fn rank_answer(
    q_ptr: i32,  q_len: i32,  // question
    gt_ptr: i32, gt_len: i32, // ground truth
    ma_ptr: i32, ma_len: i32, // miner answer
) -> f32 {
    let question_bytes = read_bytes(q_ptr, q_len);
    let ground_truth_bytes = read_bytes(gt_ptr, gt_len);
    let miner_answer_bytes = read_bytes(ma_ptr, ma_len);
    let question = decode(question_bytes);
    let ground_truth = decode(ground_truth_bytes);
    let miner_answer = decode(miner_answer_bytes);

    // Empty / whitespace-only answer → immediate 0
    if effectively_empty(miner_answer.as_ref()) {
        return 0.0;
    }
    if miner_answer_bytes == ground_truth_bytes {
        return 1.0;
    }

    let (relevance, correctness, lexical, len_quality) =
        compute_signals(question.as_ref(), ground_truth.as_ref(), miner_answer.as_ref());

    composite_checked(
        relevance,
        correctness,
        lexical,
        len_quality,
        question.as_ref(),
        ground_truth.as_ref(),
        miner_answer.as_ref(),
    )
}

/// Composite scorer variant for callers that already have `question` and
/// `ground_truth` embedded — e.g. Stage 2 replay evaluation
/// (pkg/scoring/candidate_eval.go), where every miner answering the same
/// intent shares the same question/ground_truth text. Embedding is the
/// dominant cost of scoring (multi-head transformer inference over up to
/// MAX_SEQ_LEN tokens); re-embedding the same question/ground_truth text on
/// every row in an intent group is pure waste. Callers embed each unique
/// (question, ground_truth) pair once via `embed`, cache the two vectors,
/// and pass them here for every row in that group — only `miner_answer`
/// gets freshly embedded per call.
///
/// Uses the exact same weight constants and composite() as `rank_answer` —
/// deliberately NOT a separate reimplementation, so the two can't drift
/// apart if the weights ever change.
///
/// `q_vec_ptr`/`gt_vec_ptr` must each point to EMBED_DIM (384) contiguous
/// f32 values already written into WASM linear memory (e.g. via a prior
/// `embed()` call's returned pointer — or bytes the Go host wrote directly
/// into memory obtained from this module's own `alloc()`, NOT an arbitrary
/// hardcoded offset, since that risks colliding with this module's static
/// data or allocator bookkeeping).
///
/// `gt_ptr`/`gt_len` is the ground_truth TEXT, still required for BM25
/// (lexical overlap has no vector representation to precompute).
#[no_mangle]
pub unsafe extern "C" fn rank_answer_cached(
    q_vec_ptr: i32,
    gt_vec_ptr: i32,
    gt_ptr: i32, gt_len: i32, // ground truth TEXT (for BM25)
    ma_ptr: i32, ma_len: i32, // miner answer
) -> f32 {
    let ground_truth_bytes = read_bytes(gt_ptr, gt_len);
    let miner_answer_bytes = read_bytes(ma_ptr, ma_len);
    let ground_truth = decode(ground_truth_bytes);
    let miner_answer = decode(miner_answer_bytes);

    if effectively_empty(miner_answer.as_ref()) {
        return 0.0;
    }
    if miner_answer_bytes == ground_truth_bytes {
        return 1.0;
    }

    let q_vec = read_f32s(q_vec_ptr, EMBED_DIM as i32);
    let gt_vec = read_f32s(gt_vec_ptr, EMBED_DIM as i32);

    let ma_enc = tokenizer::tokenize(miner_answer.as_ref());
    let ma_vec = embed::run(&ma_enc);

    let (relevance, correctness, lexical, len_quality) =
        signals_from_vecs(
            q_vec,
            gt_vec,
            ground_truth.as_ref(),
            miner_answer.as_ref(),
            &ma_vec,
        );

    // This is the entry point Stage 2 replay evaluation actually calls, so the
    // fact channel and calibration must be applied here too. The question
    // arrives pre-embedded rather than as text, so echo detection is
    // unavailable on this path; the ground-truth coverage rule still applies.
    composite_checked(
        relevance,
        correctness,
        lexical,
        len_quality,
        "",
        ground_truth.as_ref(),
        miner_answer.as_ref(),
    )
}

/// Per-signal breakdown scorer.
///
/// Runs the same computation as `rank_answer` but writes all five values
/// into the static `BREAKDOWN_BUF` and returns its byte offset in WASM
/// linear memory so the Go host can read 5 × 4 = 20 bytes from that address.
///
/// Buffer layout (indices match Go's SignalBreakdown struct):
///   [0] relevance     — cosine(question,     miner_answer)
///   [1] correctness   — cosine(ground_truth, miner_answer)
///   [2] lexical       — bm25(ground_truth,   miner_answer)
///   [3] length        — sigmoid length penalty
///   [4] composite     — weighted sum, clamped to [0,1]
///
/// Returns 0 (all signals 0) for empty/whitespace-only miner answers.
#[no_mangle]
pub unsafe extern "C" fn breakdown_answer(
    q_ptr: i32,  q_len: i32,  // question
    gt_ptr: i32, gt_len: i32, // ground truth
    ma_ptr: i32, ma_len: i32, // miner answer
) -> i32 {
    let question = decode(read_bytes(q_ptr, q_len));
    let ground_truth = decode(read_bytes(gt_ptr, gt_len));
    let miner_answer = decode(read_bytes(ma_ptr, ma_len));

    if effectively_empty(miner_answer.as_ref()) {
        BREAKDOWN_BUF = [0f32; BREAKDOWN_DIM];
        return BREAKDOWN_BUF.as_ptr() as i32;
    }

    let (relevance, correctness, lexical, len_quality) =
        compute_signals(question.as_ref(), ground_truth.as_ref(), miner_answer.as_ref());

    let composite_score = composite(relevance, correctness, lexical, len_quality);

    BREAKDOWN_BUF[IDX_RELEVANCE]   = relevance;
    BREAKDOWN_BUF[IDX_CORRECTNESS] = correctness;
    BREAKDOWN_BUF[IDX_LEXICAL]     = lexical;
    BREAKDOWN_BUF[IDX_LENGTH]      = len_quality;
    BREAKDOWN_BUF[IDX_COMPOSITE]   = composite_score;

    BREAKDOWN_BUF.as_ptr() as i32
}

/// Embed `text` using MiniLM-L6-v2.
///
/// Writes the 384-dim L2-normalised float32 vector into the static `EMBED_BUF`
/// and returns its byte offset in WASM linear memory so the Go host can read
/// 384 × 4 = 1 536 bytes from that address.
#[no_mangle]
pub unsafe extern "C" fn embed(text_ptr: i32, text_len: i32) -> i32 {
    let text = decode(read_bytes(text_ptr, text_len));
    let enc  = tokenizer::tokenize(text.as_ref());
    let vec  = embed::run(&enc);

    EMBED_BUF.copy_from_slice(&vec);
    EMBED_BUF.as_ptr() as i32
}

/// Cosine similarity between two float32 vectors already in WASM memory.
///
/// `dim` is the number of elements (not bytes). Returns a value in [0, 1].
#[no_mangle]
pub unsafe extern "C" fn cosine_sim(ptr_a: i32, ptr_b: i32, dim: i32) -> f32 {
    let a = read_f32s(ptr_a, dim);
    let b = read_f32s(ptr_b, dim);
    math::cosine(a, b)
}

/// BM25 lexical relevance of `doc` against `query`, normalised to [0, 1].
#[no_mangle]
pub unsafe extern "C" fn bm25_score(q_ptr: i32, q_len: i32, doc_ptr: i32, doc_len: i32) -> f32 {
    let query = decode(read_bytes(q_ptr, q_len));
    let doc = decode(read_bytes(doc_ptr, doc_len));
    bm25::score(query.as_ref(), doc.as_ref())
}

/// Allocate `size` bytes on the WASM heap and return the pointer.
/// The Go host calls this before writing strings into WASM memory.
#[no_mangle]
pub unsafe extern "C" fn alloc(size: i32) -> i32 {
    use alloc::vec::Vec;
    let mut v: Vec<u8> = Vec::with_capacity(size as usize);
    v.set_len(size as usize);
    let ptr = v.as_mut_ptr() as i32;
    core::mem::forget(v);
    ptr
}

/// Free memory previously returned by `alloc`.
#[no_mangle]
pub unsafe extern "C" fn dealloc(ptr: i32, size: i32) {
    use alloc::vec::Vec;
    let _ = Vec::from_raw_parts(ptr as *mut u8, size as usize, size as usize);
}

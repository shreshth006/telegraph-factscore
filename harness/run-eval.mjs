#!/usr/bin/env node

/**
 * Offline proxy for the node's two-stage promotion gate, plus per-class accuracy.
 *
 *   node track2/harness/run-eval.mjs --scorer cand.wasm [--against champ.wasm]
 *        [--fixtures track2/fixtures/real,track2/fixtures/synth] [--intent ALL|NAME]
 *        [--max-real-answers 12] [--workers N] [--out track2/fixtures] [--quiet]
 *
 * Stage 1 (structural, candidate only) and Stage 2 (separation vs the reference)
 * follow track2/recon/2026-08-27-track2-scorer-spec.md section 5. This is a PROXY:
 * the node's built-in benchmark is not published, so the numbers below are this
 * corpus's, not the node's. What transfers is the comparison, not the absolute.
 */

import { mkdir, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { loadScorer, utf8, rawBytes } from "./wasm-abi.mjs";
import { scoreAll, defaultWorkers } from "./score-pool.mjs";
import { loadCorpus, byIntent, mean, stddev, spearman, round } from "./corpus.mjs";
import { renderReport } from "./report.mjs";

/**
 * Gate constants. Source: the 2026-08-27 gate recon (a redacted-then-restored
 * telegraph-docs page recovered from git history, cross-checked against 1,033
 * live rejection strings). They are STRICTER than the public docs in two places
 * the public text gets wrong: margin must STRICTLY exceed the champion's (not
 * merely tie), and stddev must STRICTLY exceed 0.05. Not independently verified
 * by this harness -- see track2/GAPS.md.
 */
const GATE = {
  self_match_floor: 0.75, // rank_answer(q,gt,gt) >= max(0.75, incumbent self-match)
  stddev_floor: 0.05, // strictly greater than; 0.0381 was rejected
  spearman_floor: 0.6, // skipped when the intent has < 2 miners with history
  spearman_min_miners: 2,
  margin_absolute_floor: 0.15, // margin must also clear this in absolute terms
  wall_clock_budget_s: 600, // whole-gate budget including module load, 3-attempt cap
};

// ~15 benchmark fixtures x (self + good + bad) plus up to ~21 historical rows.
const GATE_CALL_ESTIMATE = 15 * 3 + 21;

function parseArgs(argv) {
  const args = new Map();
  for (let i = 0; i < argv.length; i += 1) {
    const token = argv[i];
    if (!token.startsWith("--")) throw new Error(`Unexpected argument: ${token}`);
    const equal = token.indexOf("=");
    if (equal > 0) {
      args.set(token.slice(0, equal), token.slice(equal + 1));
      continue;
    }
    const next = argv[i + 1];
    if (next === undefined || next.startsWith("--")) args.set(token, "true");
    else {
      args.set(token, next);
      i += 1;
    }
  }
  return args;
}

function selectAnswers(record, maxReal) {
  if (record.class !== "REAL") return record.answers;
  return record.answers.slice(0, maxReal);
}

function buildJobs(records, maxReal) {
  const jobs = [];
  const keys = [];
  records.forEach((record, ri) => {
    const unrelated = records[(ri + 1) % records.length];
    jobs.push([record.question, record.ground_truth, record.ground_truth]);
    keys.push(`${ri}:__self`);
    jobs.push([record.question, record.ground_truth, unrelated.ground_truth]);
    keys.push(`${ri}:__cross`);
    for (const answer of selectAnswers(record, maxReal)) {
      jobs.push([record.question, record.ground_truth, answer.text]);
      keys.push(`${ri}:${answer.id}`);
    }
  });
  return { jobs, keys };
}

const ADVERSARIAL = [
  ["54KB repeated text", utf8("forecast certificate vulnerability price coordinates ".repeat(1040))],
  ["100KB answer", utf8("forecast certificate vulnerability price coordinates ".repeat(2000))],
  ["emoji only", utf8("🌩️🌡️🔐📈🛰️".repeat(40))],
  ["non-Latin", utf8("東京の天気予報 · Прогноз погоды · توقعات الطقس · 기상 예보".repeat(20))],
  ["accents", utf8("température prévue · Höchstwert · señal atmosférica · 39.6438° N".repeat(20))],
  [
    "invalid UTF-8",
    rawBytes([0xff, 0xfe, 0x80, 0x81, 0xc3, 0x28, 0xe2, 0x28, 0xa1, 0xf0, 0x28, 0x8c, 0x28, 0xff]),
    true,
  ],
  ["NUL bytes", rawBytes([0x61, 0x00, 0x62, 0x00, 0x00, 0x63, 0x0a, 0x00])],
  ["one 50k token", utf8("z".repeat(50000))],
];

function stage1(scorer, records, selfScores, crossScores) {
  const checks = [];
  const exports = scorer.exportNames();
  const arity = scorer.arity();
  checks.push({
    name: "exports memory/alloc/rank_answer",
    pass: ["memory", "alloc", "rank_answer"].every((k) => exports.includes(k)),
    detail: exports.join(", "),
  });
  checks.push({
    name: "exports dealloc (node requires it; this harness does not call it)",
    pass: exports.includes("dealloc"),
    detail: exports.includes("dealloc") ? "present" : "MISSING — the node will reject the module",
  });
  checks.push({
    name: "rank_answer takes exactly 6 i32 params",
    pass: arity.rank_answer === 6,
    detail: `rank_answer/${arity.rank_answer}, alloc/${arity.alloc}, dealloc/${arity.dealloc}`,
  });

  const sample = records.filter((_, i) => i % Math.max(1, Math.floor(records.length / 5)) === 0).slice(0, 5);
  // ASCII blanks are what the docs mean by "empty or blank". Unicode blanks get
  // their own check because the champion binaries treat them differently: the
  // blank test is byte-level ASCII, so a U+00A0 / U+2003 answer falls through to
  // the scoring path and returns a small non-zero. Reported, not merged away.
  const blankGroups = [
    ["empty answer returns exactly 0.0", [["empty string", ""]]],
    [
      "ASCII whitespace-only answer returns exactly 0.0 (a 0.0007 was a recorded rejection)",
      [["3 spaces", "   "], ["newline+tab+space", "\n\t "], ["single space", " "]],
    ],
    [
      "Unicode whitespace-only answer returns exactly 0.0",
      [
        ["NBSP", "\u00a0"],
        ["NBSP+spaces+EM", "\u00a0  \u2003"],
        ["ideographic", "\u3000\u3000"],
        ["zero-width", "\u200b\u200b"],
      ],
      true, // advisory: the live champions themselves fail this, so it is not a gate condition
    ],
  ];
  for (const [name, variants, advisory] of blankGroups) {
    const results = [];
    for (const record of sample) {
      for (const [label, text] of variants) results.push([label, scorer.score(record.question, record.ground_truth, text)]);
    }
    const offenders = results.filter(([, v]) => v !== 0);
    const worst = offenders.length ? offenders.reduce((x, y) => (y[1] > x[1] ? y : x)) : null;
    checks.push({
      name,
      advisory: Boolean(advisory),
      pass: offenders.length === 0,
      detail: worst
        ? `${results.length} probes, ${offenders.length} non-zero, worst "${worst[0]}" = ${worst[1]}`
        : `${results.length} probes, all exactly 0`,
    });
  }

  let failures = 0;
  let worstGap = Infinity;
  records.forEach((record, i) => {
    const gap = selfScores[i] - crossScores[i];
    if (!(selfScores[i] > crossScores[i])) failures += 1;
    if (gap < worstGap) worstGap = gap;
  });
  checks.push({
    name: "self-match beats unrelated cross-match on every fixture",
    pass: failures === 0,
    detail: `${records.length - failures}/${records.length} fixtures, smallest gap ${round(worstGap)}`,
  });

  // Serial, in-process latency on ordinary inputs. The pool's wall time is
  // distorted by memory contention between workers (these modules carry ~24 MB
  // of static tables), so the gate's wall-clock budget must be projected from a
  // single-threaded measurement, not from worker-seconds.
  const timed = records.slice(0, 8);
  const start = process.hrtime.bigint();
  for (const record of timed) scorer.score(record.question, record.ground_truth, record.answers[0]?.text ?? record.ground_truth);
  const serialSecondsPerCall = Number(process.hrtime.bigint() - start) / 1e9 / Math.max(1, timed.length);

  const adversarial = [];
  for (const [name, bytes, advisory] of ADVERSARIAL) {
    const record = records[0];
    try {
      // A deliberately trapping input must not leave allocator/model state
      // behind for the next probe. The node evaluates retries in fresh module
      // instances, and each adversarial case is an independent assertion.
      scorer.reset();
      const t = process.hrtime.bigint();
      const value = scorer.scoreBytes(utf8(record.question), utf8(record.ground_truth), bytes);
      adversarial.push({
        name,
        advisory: Boolean(advisory),
        ok: Number.isFinite(value) && value >= 0 && value <= 1,
        value: round(value),
        seconds: round(Number(process.hrtime.bigint() - t) / 1e9, 2),
      });
    } catch (error) {
      adversarial.push({ name, advisory: Boolean(advisory), ok: false, error: error.message });
    }
  }
  checks.push({
    name: "no crash on adversarial input",
    pass: adversarial.filter((a) => !a.advisory).every((a) => a.ok),
    detail: adversarial
      .map((a) => `${a.name}${a.advisory ? "[advisory]" : ""}=${a.error ? `THREW(${a.error})` : a.value}`)
      .join("  "),
  });

  // Advisory checks are reported but do not decide the verdict: they are stricter
  // than the node's own gate, and the live champions fail some of them.
  const projected = serialSecondsPerCall * GATE_CALL_ESTIMATE + 10;
  checks.push({
    name: `whole gate fits the ${GATE.wall_clock_budget_s}s wall clock`,
    pass: projected <= GATE.wall_clock_budget_s,
    detail:
      `${round(serialSecondsPerCall, 3)}s/call serial x ~${GATE_CALL_ESTIMATE} calls + 10s load ` +
      `= ~${Math.round(projected)}s of ${GATE.wall_clock_budget_s}s` +
      `${projected > GATE.wall_clock_budget_s * 0.5 ? "   (over half the budget)" : ""}`,
  });
  const slowest = adversarial.filter((a) => a.seconds !== undefined).sort((a, b) => b.seconds - a.seconds)[0];
  if (slowest) {
    checks.push({
      name: "worst adversarial input stays well inside the budget",
      pass: slowest.seconds * 3 <= GATE.wall_clock_budget_s,
      detail: `slowest "${slowest.name}" took ${slowest.seconds}s`,
    });
  }

  return {
    exports,
    checks,
    adversarial,
    serial_seconds_per_call: round(serialSecondsPerCall, 3),
    projected_gate_seconds: Math.round(projected),
    pass: checks.filter((c) => !c.advisory).every((c) => c.pass),
  };
}

function pairStats(records, indexOf, scores, maxReal) {
  const byClass = new Map();
  records.forEach((record, ri) => {
    const allowed = new Set(selectAnswers(record, maxReal).map((a) => a.id));
    for (const [better, worse] of record.pairs ?? []) {
      if (!allowed.has(better) || !allowed.has(worse)) continue;
      const a = scores[indexOf.get(`${ri}:${better}`)];
      const b = scores[indexOf.get(`${ri}:${worse}`)];
      if (a === undefined || b === undefined) continue;
      const klass = record.class;
      if (!byClass.has(klass)) byClass.set(klass, { pairs: 0, wins: 0, margins: [] });
      const bucket = byClass.get(klass);
      bucket.pairs += 1;
      if (a > b) bucket.wins += 1;
      bucket.margins.push(a - b);
    }
  });
  return byClass;
}

function constraintStats(records, indexOf, scores, maxReal) {
  const byClass = new Map();
  records.forEach((record, ri) => {
    const allowed = new Set(selectAnswers(record, maxReal).map((a) => a.id));
    for (const constraint of record.constraints ?? []) {
      const ids = (constraint.ids ?? []).filter((id) => allowed.has(id));
      const values = ids.map((id) => scores[indexOf.get(`${ri}:${id}`)]).filter((v) => v !== undefined);
      if (values.length < 2) continue;
      const spread = Math.max(...values) - Math.min(...values);
      const klass = record.class;
      if (!byClass.has(klass)) byClass.set(klass, { total: 0, satisfied: 0, worst: 0, tolerance: constraint.tolerance });
      const bucket = byClass.get(klass);
      bucket.total += 1;
      if (spread <= constraint.tolerance) bucket.satisfied += 1;
      bucket.worst = Math.max(bucket.worst, spread);
    }
  });
  return byClass;
}

function summarise(byClass) {
  const out = {};
  let pairs = 0;
  let wins = 0;
  const margins = [];
  for (const [klass, bucket] of byClass) {
    out[klass] = {
      pairs: bucket.pairs,
      wins: bucket.wins,
      accuracy: round(bucket.wins / bucket.pairs, 4),
      mean_margin: round(mean(bucket.margins)),
    };
    pairs += bucket.pairs;
    wins += bucket.wins;
    margins.push(...bucket.margins);
  }
  return { per_class: out, pairs, wins, accuracy: pairs ? round(wins / pairs, 4) : null, mean_margin: round(mean(margins)) };
}

/** Distinct miners with scoring history in this intent's REAL fixtures. */
export function distinctMiners(records) {
  const slugs = new Set();
  for (const record of records) {
    if (record.class !== "REAL") continue;
    for (const answer of record.answers) if (answer.meta?.miner_slug) slugs.add(answer.meta.miner_slug);
  }
  return slugs.size;
}

function intentMetrics(records, indexOf, scores, maxReal) {
  const selfs = records.map((_, ri) => scores[indexOf.get(`${ri}:__self`)]);
  const answerScores = [];
  const spreads = [];
  const good = [];
  const bad = [];
  records.forEach((record, ri) => {
    for (const answer of selectAnswers(record, maxReal)) {
      const value = scores[indexOf.get(`${ri}:${answer.id}`)];
      if (value === undefined) continue;
      answerScores.push(value);
      if (answer.quality === 1) good.push(value);
      else if (answer.quality === 0) bad.push(value);
    }
    const values = selectAnswers(record, maxReal)
      .map((a) => scores[indexOf.get(`${ri}:${a.id}`)])
      .filter((v) => v !== undefined);
    if (values.length > 1) spreads.push(Math.max(...values) - Math.min(...values));
  });
  const pairs = summarise(pairStats(records, indexOf, scores, maxReal));
  const separation = good.length && bad.length ? mean(good) - mean(bad) : null;
  return {
    fixtures: records.length,
    worst_self_match: round(Math.min(...selfs)),
    mean_self_match: round(mean(selfs)),
    score_stddev: round(stddev(answerScores)),
    mean_score: round(mean(answerScores)),
    mean_discrimination: round(mean(spreads)),
    // The node's candidate_margin is mean(good) - mean(bad) over its benchmark.
    // `separation` is that definition on this corpus; `mean_margin` is the
    // per-pair average, which weights fixtures with more pairs differently.
    separation: round(separation),
    mean_good: round(mean(good)),
    mean_bad: round(mean(bad)),
    labelled: { good: good.length, bad: bad.length },
    wins: pairs.wins,
    pairs: pairs.pairs,
    accuracy: pairs.accuracy,
    mean_margin: pairs.mean_margin,
    per_class: pairs.per_class,
    constraints: Object.fromEntries(
      [...constraintStats(records, indexOf, scores, maxReal)].map(([k, v]) => [
        k,
        { total: v.total, satisfied: v.satisfied, tolerance: v.tolerance, worst_spread: round(v.worst) },
      ]),
    ),
  };
}

/**
 * The node's Stage-2 conditions applied to this corpus. Every comparison that
 * needs an incumbent is skipped (not passed) when --against was not supplied.
 */
function verdict(candidate, against, spear, miners) {
  const checks = [];
  const add = (name, state, detail) => checks.push({ name, state, detail });

  add(
    "A  score_stddev > 0.05 (strict)",
    candidate.score_stddev === null ? "SKIP" : candidate.score_stddev > GATE.stddev_floor ? "PASS" : "FAIL",
    `candidate ${candidate.score_stddev}${against ? ` | reference ${against.score_stddev}` : ""}`,
  );

  const selfBar = Math.max(GATE.self_match_floor, against ? against.worst_self_match : 0);
  add(
    "B  worst_self_match >= max(0.75, incumbent)",
    candidate.worst_self_match >= selfBar ? "PASS" : "FAIL",
    `candidate ${candidate.worst_self_match} vs bar ${round(selfBar)}${against ? ` (incumbent ${against.worst_self_match})` : " (no incumbent supplied)"}`,
  );

  if (!against) add("C  Spearman >= 0.60 vs champion on real traffic", "SKIP", "no --against scorer supplied");
  else if (miners < GATE.spearman_min_miners)
    add("C  Spearman >= 0.60 vs champion on real traffic", "SKIP", `only ${miners} miner(s) with history in this intent`);
  else if (spear?.rho === null || spear === null)
    add("C  Spearman >= 0.60 vs champion on real traffic", "SKIP", `undefined (n=${spear?.n ?? 0}, a series was constant)`);
  else add("C  Spearman >= 0.60 vs champion on real traffic", spear.rho >= GATE.spearman_floor ? "PASS" : "FAIL", `rho ${spear.rho} over n=${spear.n} real answers, ${miners} miners`);

  if (candidate.separation === null) add("D1 margin > champion_margin (strict)", "SKIP", "no labelled good/bad answers in scope");
  else if (!against) add("D1 margin > champion_margin (strict)", "SKIP", `candidate margin ${candidate.separation}, no incumbent supplied`);
  else
    add(
      "D1 margin > champion_margin (strict)",
      candidate.separation > against.separation ? "PASS" : "FAIL",
      `candidate ${candidate.separation} vs reference ${against.separation} (delta ${round(candidate.separation - against.separation)})`,
    );

  add(
    "D2 margin >= 0.15 absolute",
    candidate.separation === null ? "SKIP" : candidate.separation >= GATE.margin_absolute_floor ? "PASS" : "FAIL",
    `candidate ${candidate.separation}`,
  );

  add(
    "D3 candidate_wins >= champion_wins (tie allowed)",
    !against ? "SKIP" : candidate.wins >= against.wins ? "PASS" : "FAIL",
    `candidate ${candidate.wins}/${candidate.pairs}${against ? ` vs reference ${against.wins}/${against.pairs}` : ""}`,
  );

  return { checks, pass: checks.every((c) => c.state !== "FAIL") };
}

function realSpearman(records, indexOf, candScores, againstScores, maxReal) {
  const xs = [];
  const ys = [];
  records.forEach((record, ri) => {
    if (record.class !== "REAL") return;
    for (const answer of selectAnswers(record, maxReal)) {
      const i = indexOf.get(`${ri}:${answer.id}`);
      if (i === undefined) continue;
      xs.push(candScores[i]);
      ys.push(againstScores[i]);
    }
  });
  return { n: xs.length, rho: round(spearman(xs, ys), 4) };
}

function exhibits(records, indexOf, candScores, againstScores, maxReal) {
  const rows = [];
  const at = (ri, id) => {
    const i = indexOf.get(`${ri}:${id}`);
    return i === undefined ? null : { candidate: round(candScores[i]), against: againstScores ? round(againstScores[i]) : null };
  };
  records.forEach((record, ri) => {
    const ids = new Set(record.answers.map((a) => a.id));
    if (record.class === "REAL-PARROT") {
      // The headline reproduction: a contentless echo of a REAL question, scored
      // against that question's REAL ground truth, next to what miners actually
      // answered. Real ground truths are long hedged prose, so a data payload
      // that does not echo the question shares almost none of their wording.
      const scores = { "prefix-parrot (no data)": at(ri, "prefix-parrot"), "parrot + real data": at(ri, "parrot-plus-data") };
      const live = {};
      for (const answer of record.answers) {
        if (!answer.meta) continue;
        scores[`${answer.id} (real data)`] = at(ri, answer.id);
        live[answer.id] = answer.meta.live_score;
      }
      rows.push({ kind: "REAL-PARROT", intent: record.intent, fixture: record.id, scores, live_scores: live });
      return;
    }
    if (ids.has("prefix-parrot")) {
      rows.push({
        kind: "PREFIX-PARROT",
        intent: record.intent,
        fixture: record.id,
        scores: {
          "prefix-parrot (no data)": at(ri, "prefix-parrot"),
          "correct-prose (echo + data)": at(ri, "correct-prose"),
          "correct-nonecho (data, no echo)": at(ri, "correct-nonecho"),
        },
      });
    }
    if (ids.has("refusal-echo")) {
      rows.push({
        kind: "REFUSAL-ECHO",
        intent: record.intent,
        fixture: record.id,
        scores: { "refusal-echo": at(ri, "refusal-echo"), "correct-prose": at(ri, "correct-prose") },
      });
    }
    if (record.class === "REAL") {
      const chosen = selectAnswers(record, maxReal);
      const refusals = chosen.filter((a) => a.meta?.looks_like_refusal);
      const data = chosen.filter((a) => !a.meta?.looks_like_refusal);
      if (refusals.length && data.length) {
        const best = (list, get) => list.reduce((x, y) => (get(y) > get(x) ? y : x));
        const scoreOf = (a) => candScores[indexOf.get(`${ri}:${a.id}`)];
        const topRefusal = best(refusals, scoreOf);
        const topData = best(data, scoreOf);
        if (scoreOf(topRefusal) > scoreOf(topData)) {
          rows.push({
            kind: "REAL-REFUSAL-BEATS-DATA",
            intent: record.intent,
            fixture: record.id,
            scores: { [`${topRefusal.id} (refusal-shaped)`]: at(ri, topRefusal.id), [`${topData.id} (data)`]: at(ri, topData.id) },
            live_scores: { [topRefusal.id]: topRefusal.meta.live_score, [topData.id]: topData.meta.live_score },
          });
        }
      }
    }
  });
  return rows;
}

async function main() {
  const args = parseArgs(process.argv.slice(2));
  const scorerPath = args.get("--scorer");
  if (!scorerPath || scorerPath === "true") throw new Error("--scorer <path-to-wasm> is required");
  const againstPath = args.get("--against") && args.get("--against") !== "true" ? args.get("--against") : null;
  const dirs = (args.get("--fixtures") ?? "track2/fixtures/real,track2/fixtures/synth,track2/fixtures/probe").split(",");
  const intent = args.get("--intent") ?? "all";
  const maxReal = Number(args.get("--max-real-answers") ?? 12);
  const workers = Number(args.get("--workers") ?? defaultWorkers());
  const outDir = args.get("--out") ?? "track2/fixtures";
  const quiet = args.has("--quiet");

  const candidate = await loadScorer(scorerPath, "candidate");
  const against = againstPath ? await loadScorer(againstPath, "against") : null;
  const { records, files } = await loadCorpus(dirs, intent);
  if (records.length === 0) throw new Error(`No fixtures matched ${dirs.join(",")} intent=${intent}`);

  const { jobs, keys } = buildJobs(records, maxReal);
  const indexOf = new Map(keys.map((k, i) => [k, i]));
  const started = Date.now();
  const tick = (label) => (done, total) => {
    if (!quiet && (done % 25 === 0 || done === total)) {
      process.stderr.write(`\r  ${label}: ${done}/${total} calls (${Math.round((Date.now() - started) / 1000)}s)   `);
    }
  };

  if (!quiet) console.error(`corpus: ${records.length} fixtures, ${jobs.length} scoring calls per module, ${workers} workers`);
  const t0 = Date.now();
  const candScores = await scoreAll(scorerPath, "candidate", jobs, workers, tick("candidate"));
  const t1 = Date.now();
  const againstScores = againstPath ? await scoreAll(againstPath, "against", jobs, workers, tick("against")) : null;
  const t2 = Date.now();
  if (!quiet) process.stderr.write("\n");

  // The node runs its whole gate inside a 10-minute wall clock, serially. Our
  // wall time is fanned across workers, so cost is reported as worker-seconds
  // per call and projected onto the node's serial call count.
  const timing = (ms, count) => {
    const perCall = count ? (ms / 1000) * workers / count : null;
    const nodeCalls = GATE_CALL_ESTIMATE;
    const projected = perCall === null ? null : perCall * nodeCalls + 10;
    return {
      wall_seconds: Math.round(ms / 1000),
      worker_seconds_per_call: round(perCall, 3),
      projected_node_gate_seconds: projected === null ? null : Math.round(projected),
      budget_seconds: GATE.wall_clock_budget_s,
      at_risk: projected !== null && projected > GATE.wall_clock_budget_s * 0.5,
    };
  };

  const selfScores = records.map((_, ri) => candScores[indexOf.get(`${ri}:__self`)]);
  const crossScores = records.map((_, ri) => candScores[indexOf.get(`${ri}:__cross`)]);
  const stageOne = stage1(candidate, records, selfScores, crossScores);

  const intents = byIntent(records);
  const gate = {};
  for (const [name, group] of intents) {
    const rows = group.map((r) => records.indexOf(r));
    const sub = { records: group, indexOf: new Map(), };
    // remap indices so per-intent metrics can reuse the shared score arrays
    group.forEach((record, gi) => {
      const ri = rows[gi];
      sub.indexOf.set(`${gi}:__self`, indexOf.get(`${ri}:__self`));
      sub.indexOf.set(`${gi}:__cross`, indexOf.get(`${ri}:__cross`));
      for (const answer of record.answers) {
        const key = indexOf.get(`${ri}:${answer.id}`);
        if (key !== undefined) sub.indexOf.set(`${gi}:${answer.id}`, key);
      }
    });
    const cand = intentMetrics(group, sub.indexOf, candScores, maxReal);
    const ref = againstScores ? intentMetrics(group, sub.indexOf, againstScores, maxReal) : null;
    const spear = againstScores ? realSpearman(group, sub.indexOf, candScores, againstScores, maxReal) : null;
    const miners = distinctMiners(group);
    gate[name] = { candidate: cand, against: ref, spearman: spear, distinct_miners: miners, verdict: verdict(cand, ref, spear, miners) };
  }
  {
    const cand = intentMetrics(records, indexOf, candScores, maxReal);
    const ref = againstScores ? intentMetrics(records, indexOf, againstScores, maxReal) : null;
    const spear = againstScores ? realSpearman(records, indexOf, candScores, againstScores, maxReal) : null;
    const miners = distinctMiners(records);
    gate.ALL = { candidate: cand, against: ref, spearman: spear, distinct_miners: miners, verdict: verdict(cand, ref, spear, miners) };
  }

  const report = {
    generated_at: new Date().toISOString(),
    command: `node ${process.argv.slice(1).join(" ")}`,
    thresholds: {
      ...GATE,
      source: "2026-08-27 gate recon: restored telegraph-docs page from git history + 1,033 live rejection strings",
      status: "NOT independently verified by this harness — see track2/GAPS.md",
    },
    candidate: { path: scorerPath, sha256: candidate.sha256, bytes: candidate.sizeBytes, timing: timing(t1 - t0, jobs.length) },
    against: against ? { path: againstPath, sha256: against.sha256, bytes: against.sizeBytes, timing: timing(t2 - t1, jobs.length) } : null,
    corpus: { dirs, intent, files, fixtures: records.length, calls_per_module: jobs.length, max_real_answers: maxReal, workers },
    stage1: stageOne,
    gate_proxy: gate,
    exhibits: exhibits(records, indexOf, candScores, againstScores, maxReal),
    runtime_seconds: Math.round((Date.now() - started) / 1000),
  };

  const text = renderReport(report);
  console.log(text);
  await mkdir(outDir, { recursive: true });
  const stamp = report.generated_at.replace(/[:.]/g, "-");
  const path = join(outDir, `report-${stamp}.json`);
  await writeFile(path, `${JSON.stringify(report, null, 2)}\n`);
  console.log(`\nreport written: ${path}`);
  if (!stageOne.pass) process.exitCode = 1;
}

main().catch((error) => {
  console.error(error instanceof Error ? error.stack : String(error));
  process.exitCode = 1;
});

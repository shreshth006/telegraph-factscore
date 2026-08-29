#!/usr/bin/env node

/**
 * Recover the score immediately before the hybrid scorer's final logistic
 * calibration, then evaluate alternative calibration constants without
 * rebuilding or changing factual ordering.
 *
 * This is intentionally limited to the hybrid artifact whose current
 * calibration constants are supplied on the command line. Scores saturated
 * to exactly zero or one are retained at those endpoints; the report counts
 * them so reviewers can see where inversion has lost information.
 */

import { loadCorpus } from "./corpus.mjs";
import { scoreAll, defaultWorkers } from "./score-pool.mjs";

function parseArgs(argv) {
  const values = new Map();
  for (let index = 0; index < argv.length; index += 1) {
    const key = argv[index];
    if (!key.startsWith("--")) continue;
    const next = argv[index + 1];
    if (next && !next.startsWith("--")) {
      values.set(key, next);
      index += 1;
    } else {
      values.set(key, true);
    }
  }
  return values;
}

function required(args, key) {
  const value = args.get(key);
  if (!value || value === true) throw new Error(`${key} is required`);
  return value;
}

function numberArg(args, key, fallback) {
  const value = Number(args.get(key) ?? fallback);
  if (!Number.isFinite(value)) throw new Error(`${key} must be a finite number`);
  return value;
}

function range(start, end, step) {
  if (step <= 0 || end < start) throw new Error("invalid sweep range");
  const values = [];
  for (let value = start; value <= end + step / 10; value += step) values.push(value);
  return values;
}

function inverseLogistic(score, center, sharpness) {
  if (score <= 0) return 0;
  if (score >= 1) return 1;
  return center + Math.log(score / (1 - score)) / sharpness;
}

function calibrate(score, center, sharpness) {
  if (score <= 0) return 0;
  if (score >= 1) return 1;
  return 1 / (1 + Math.exp(-(score - center) * sharpness));
}

function round(value, places = 8) {
  const factor = 10 ** places;
  return Math.round(value * factor) / factor;
}

function summarize(pairs, center, sharpness) {
  const scored = pairs.map((pair) => {
    const good = calibrate(pair.goodRaw, center, sharpness);
    const bad = calibrate(pair.badRaw, center, sharpness);
    return { ...pair, good, bad, margin: good - bad };
  });
  const margins = scored.map((pair) => pair.margin);
  return {
    center: round(center, 4),
    sharpness: round(sharpness, 2),
    wins: margins.filter((margin) => margin > 1e-7).length,
    ties: margins.filter((margin) => Math.abs(margin) <= 1e-7).length,
    losses: margins.filter((margin) => margin < -1e-7).length,
    mean_margin: round(margins.reduce((sum, value) => sum + value, 0) / margins.length),
    mean_good: round(scored.reduce((sum, pair) => sum + pair.good, 0) / scored.length),
    mean_bad: round(scored.reduce((sum, pair) => sum + pair.bad, 0) / scored.length),
    worst_margin: round(Math.min(...margins)),
  };
}

async function main() {
  const args = parseArgs(process.argv.slice(2));
  const scorer = required(args, "--scorer");
  const intent = args.get("--intent") ?? "IP_GEOLOCATION";
  const fixtureDirs = String(args.get("--fixtures") ?? "fixtures/synth").split(",");
  const currentCenter = numberArg(args, "--current-center", 0.45);
  const currentSharpness = numberArg(args, "--current-sharpness", 80);
  const centerStart = numberArg(args, "--center-start", 0.38);
  const centerEnd = numberArg(args, "--center-end", 0.52);
  const centerStep = numberArg(args, "--center-step", 0.005);
  const sharpnesses = String(args.get("--sharpnesses") ?? "40,50,60,70,80,90,100,110,120,140,160,200")
    .split(",")
    .map(Number);
  if (sharpnesses.some((value) => !Number.isFinite(value) || value <= 0)) {
    throw new Error("--sharpnesses must be a comma-separated list of positive numbers");
  }

  const { records, files } = await loadCorpus(fixtureDirs, intent);
  const jobs = [];
  const pairMetadata = [];
  for (const record of records) {
    for (const [goodId, badId] of record.pairs ?? []) {
      const good = record.answers.find((answer) => answer.id === goodId);
      const bad = record.answers.find((answer) => answer.id === badId);
      jobs.push([record.question, record.ground_truth, good.text], [record.question, record.ground_truth, bad.text]);
      pairMetadata.push({ fixture: record.id, class: record.class, good_id: goodId, bad_id: badId });
    }
  }
  if (jobs.length === 0) throw new Error(`no ordered pairs found for ${intent}`);

  const workers = numberArg(args, "--workers", defaultWorkers());
  const scores = await scoreAll(scorer, "candidate", jobs, workers);
  const pairs = pairMetadata.map((metadata, index) => {
    const goodScore = scores[index * 2];
    const badScore = scores[index * 2 + 1];
    return {
      ...metadata,
      good_score: goodScore,
      bad_score: badScore,
      goodRaw: inverseLogistic(goodScore, currentCenter, currentSharpness),
      badRaw: inverseLogistic(badScore, currentCenter, currentSharpness),
    };
  });

  const sweep = [];
  for (const center of range(centerStart, centerEnd, centerStep)) {
    for (const sharpness of sharpnesses) sweep.push(summarize(pairs, center, sharpness));
  }
  sweep.sort((left, right) =>
    right.wins - left.wins ||
    left.losses - right.losses ||
    right.mean_margin - left.mean_margin ||
    right.worst_margin - left.worst_margin,
  );

  const current = summarize(pairs, currentCenter, currentSharpness);
  const report = {
    generated_at: new Date().toISOString(),
    scorer,
    intent,
    fixtures: records.length,
    ordered_pairs: pairs.length,
    source_files: files,
    inversion: { current_center: currentCenter, current_sharpness: currentSharpness },
    saturated_scores: scores.filter((score) => score === 0 || score === 1).length,
    current,
    top_candidates: sweep.slice(0, 30),
    non_wins: pairs
      .filter((pair) => pair.good_score <= pair.bad_score)
      .map((pair) => ({
        fixture: pair.fixture,
        class: pair.class,
        good_id: pair.good_id,
        bad_id: pair.bad_id,
        good_score: round(pair.good_score),
        bad_score: round(pair.bad_score),
      })),
  };
  console.log(JSON.stringify(report, null, 2));
}

main().catch((error) => {
  console.error(error instanceof Error ? error.stack : String(error));
  process.exitCode = 1;
});

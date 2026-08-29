#!/usr/bin/env node

/**
 * Worker pool for scoring a job list against one WASM module.
 *
 * A single rank_answer call on a champion binary costs 0.3-1.5 s (measured,
 * 2026-08-27), so a whole-corpus run is minutes of pure WASM. Jobs are pure
 * functions of (question, ground_truth, answer), so fanning them across workers
 * changes nothing about the result: chunks are assigned round-robin by index and
 * merged back by index, making the output identical for any worker count.
 *
 * This module is its own worker entry point.
 */

import { Worker, isMainThread, parentPort, workerData } from "node:worker_threads";
import { cpus } from "node:os";
import { fileURLToPath } from "node:url";
import { loadScorer } from "./wasm-abi.mjs";

const SELF = fileURLToPath(import.meta.url);

if (!isMainThread && workerData?.wasmPath) {
  const scorer = await loadScorer(workerData.wasmPath, workerData.label);
  parentPort.on("message", (message) => {
    if (message.type === "stop") {
      parentPort.close();
      return;
    }
    const scores = new Array(message.jobs.length);
    for (let i = 0; i < message.jobs.length; i += 1) {
      const [question, groundTruth, answer] = message.jobs[i];
      scores[i] = scorer.scoreStage2(question, groundTruth, answer);
    }
    parentPort.postMessage({ type: "done", indices: message.indices, scores });
  });
  parentPort.postMessage({ type: "ready" });
}

export function defaultWorkers() {
  return Math.max(1, Math.min(8, (cpus()?.length ?? 2) - 1));
}

/**
 * jobs: [ [question, groundTruth, answer], ... ]  ->  Float64Array of scores, same order.
 */
export async function scoreAll(wasmPath, label, jobs, workerCount, onProgress) {
  if (jobs.length === 0) return [];
  const count = Math.max(1, Math.min(workerCount, jobs.length));
  const buckets = Array.from({ length: count }, () => ({ indices: [], jobs: [] }));
  jobs.forEach((job, index) => {
    const bucket = buckets[index % count];
    bucket.indices.push(index);
    bucket.jobs.push(job);
  });

  const scores = new Array(jobs.length);
  let completed = 0;
  await Promise.all(
    buckets.map(
      (bucket) =>
        new Promise((resolve, reject) => {
          if (bucket.jobs.length === 0) return resolve();
          const worker = new Worker(SELF, { workerData: { wasmPath, label } });
          worker.on("message", (message) => {
            if (message.type === "ready") {
              worker.postMessage({ type: "jobs", indices: bucket.indices, jobs: bucket.jobs });
              return;
            }
            message.indices.forEach((index, i) => {
              scores[index] = message.scores[i];
            });
            completed += message.indices.length;
            if (onProgress) onProgress(completed, jobs.length);
            worker.postMessage({ type: "stop" });
            worker.terminate().then(resolve, resolve);
          });
          worker.on("error", reject);
        }),
    ),
  );
  return scores;
}

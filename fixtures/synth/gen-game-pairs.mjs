#!/usr/bin/env node

/**
 * Build fluent GAME_RESULT counterfactuals from captured Telegraph traffic.
 * Each negative changes one decisive slot while retaining the surrounding
 * prose: winner, scoreline, or year. This avoids rewarding a scorer merely
 * for spotting broken or off-topic text.
 */

import { createHash } from "node:crypto";
import { readFileSync, writeFileSync } from "node:fs";

const source = JSON.parse(readFileSync("../real/GAME_RESULT.json", "utf8"));

function swap(text, left, right) {
  const marker = "__TELEGRAPH_ENTITY_SWAP__";
  return text.split(left).join(marker).split(right).join(left).split(marker).join(right);
}

function wrongWinner(id, text) {
  if (id === "game_result-real-07") return swap(text, "Chelsea", "Paris Saint-Germain");
  if (id === "game_result-real-11") return swap(text, "Paris Saint-Germain", "Arsenal");
  if (id === "game_result-real-13") return swap(text, "Paris Saint-Germain", "Inter Milan");
  if (["game_result-real-05", "game_result-real-14"].includes(id)) {
    return swap(text, "Argentina", "Brazil");
  }
  return swap(text, "Spain", "Argentina");
}

function wrongScore(text) {
  const matches = [...text.matchAll(/\b(\d+)\s*([-–])\s*(\d+)\b/g)];
  const selected = [...matches].reverse().find((match) => match[1] !== match[3]) ?? matches.at(-1);
  if (!selected || selected.index === undefined) return `${text} The final score was 2-1.`;
  const replacement = `${selected[3]}${selected[2]}${selected[1]}`;
  return text.slice(0, selected.index) + replacement + text.slice(selected.index + selected[0].length);
}

function wrongYear(text) {
  if (text.includes("2026")) return text.replaceAll("2026", "2024");
  if (text.includes("2025")) return text.replaceAll("2025", "2024");
  return `${text} This result occurred in 2024.`;
}

const fixtures = source.fixtures.map((record, index) => {
  const truth = record.ground_truth;
  const answers = [
    { id: "correct", text: truth, quality: 1, note: "captured ground truth" },
    {
      id: "wrong-winner",
      text: wrongWinner(record.id, truth),
      quality: 0,
      note: "winner and opponent swapped; all other facts retained",
    },
    {
      id: "wrong-score",
      text: wrongScore(truth),
      quality: 0,
      note: "scoreline reversed; all other facts retained",
    },
    {
      id: "wrong-year",
      text: wrongYear(truth),
      quality: 0,
      note: "event year shifted; all other facts retained",
    },
    {
      id: "refusal",
      text: "I cannot determine the winner or final score from the information available.",
      quality: 0,
      note: "content-free refusal control",
    },
  ];
  return {
    id: `game_result-cleanpair-${String(index + 1).padStart(2, "0")}`,
    intent: "GAME_RESULT",
    class: "CLEAN-PAIR",
    rationale:
      "Fluent one-slot counterfactuals test whether the scorer reads the winner, scoreline, and date.",
    question: record.question,
    ground_truth: truth,
    answers,
    pairs: answers.filter((answer) => answer.quality === 0).map((answer) => ["correct", answer.id]),
    constraints: [],
    provenance: {
      source: "scores-api-derived",
      source_fixture: record.id,
      generator: "gen-game-pairs.mjs",
      created: "2026-08-29",
    },
  };
});

const body = {
  generator: "gen-game-pairs.mjs",
  generated_from: "fixtures/real/GAME_RESULT.json",
  corpus_version: createHash("sha256").update(JSON.stringify(fixtures)).digest("hex").slice(0, 16),
  fixtures,
};
writeFileSync("GAME_RESULT.json", `${JSON.stringify(body, null, 2)}\n`);
console.log(
  `wrote ${fixtures.length} fixtures, ${fixtures.reduce((sum, fixture) => sum + fixture.pairs.length, 0)} pairs, corpus_version ${body.corpus_version}`,
);

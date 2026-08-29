// Verifies a built scorer .wasm against the node's Stage-1 structural gate.
//
// Instantiates with EMPTY imports (proving the module is freestanding), asserts
// the exact six-i32 ABI, and reproduces every Stage-1 trap seen in a live
// rejection (recon/2026-08-27-node-gate-analysis.md §5), plus the hand cases
// that encode the scoring thesis.
//
//   node verify.mjs dist/ip_geolocation.wasm

import { readFile } from "node:fs/promises";

const wasmPath = process.argv[2] ?? "dist/generic.wasm";
const bytes = await readFile(wasmPath);

// Empty import object: instantiation throws if the module imports anything.
const { module, instance } = await WebAssembly.instantiate(bytes, {});
const imports = WebAssembly.Module.imports(module);
const ex = instance.exports;

let failures = 0;
const check = (ok, label, detail = "") => {
  if (!ok) failures++;
  console.log(`${ok ? "PASS" : "FAIL"}  ${label}${detail ? "  " + detail : ""}`);
  return ok;
};

console.log(`module:     ${wasmPath}`);
console.log(`size:       ${bytes.length} bytes`);
const tag = Buffer.from(bytes).toString("latin1").match(/(IP_GEOLOCATION|STORM_ALERT|GENERIC)/);
console.log(`intent tag: ${tag ? tag[1] : "(none)"}`);
console.log("");

console.log("--- Stage 1: structure ---");
check(imports.length === 0, "zero imports (freestanding)", JSON.stringify(imports));
for (const n of ["alloc", "dealloc", "rank_answer", "memory"]) {
  check(n in ex, `exports ${n}`);
}
// A 3-param rank_answer was rejected live; the host passes exactly six.
check(ex.rank_answer.length === 6, "rank_answer takes exactly 6 params", `got ${ex.rank_answer.length}`);
check(ex.alloc.length === 1, "alloc takes 1 param");
check(ex.dealloc.length === 2, "dealloc takes 2 params");

const enc = new TextEncoder();
// Mirrors telegraph-examples/wasm-scoring-module/go-tester/main.go: for an empty
// string the host does NOT call alloc, it passes ptr=0, len=0.
function put(s) {
  return putBytes(enc.encode(s));
}
function putBytes(b) {
  if (b.length === 0) return [0, 0];
  const ptr = ex.alloc(b.length);
  if (ptr === 0) throw new Error("alloc returned 0");
  new Uint8Array(ex.memory.buffer, ptr, b.length).set(b);
  return [ptr, b.length];
}
const score = (q, gt, ma) => ex.rank_answer(...put(q), ...put(gt), ...put(ma));
const scoreRawAnswer = (q, gt, ma) => ex.rank_answer(...put(q), ...put(gt), ...putBytes(ma));

console.log("");
console.log("--- Stage 1: traps ---");
const Q = "Can you look up the geolocation details for the IP address 142.251.42.174 and provide the country, city, and ISP information?";
const GT = "The IP address 142.251.42.174 is associated with Google LLC and is located in the United States. The ISP is clearly identified as Google LLC.";

const empty = score(Q, GT, "");
check(Object.is(empty, 0), "empty answer is EXACTLY 0.0", `got ${empty}`);
for (const [label, ws] of [["spaces", "   "], ["tab/CR/LF", " \t\r\n "], ["single tab", "\t"]]) {
  const v = score(Q, GT, ws);
  check(Object.is(v, 0), `whitespace-only (${label}) is EXACTLY 0.0`, `got ${v}`);
}
for (const [label, ws] of [
  ["non-breaking space", "\u00a0"],
  ["zero-width space", "\u200b"],
  ["word joiner", "\u2060"],
  ["byte-order mark", "\ufeff"],
]) {
  const v = score(Q, GT, ws);
  check(Object.is(v, 0), `Unicode-empty (${label}) is EXACTLY 0.0`, `got ${v}`);
}

const selfMatch = score(Q, GT, GT);
const cross = score(Q, GT, "Bananas grow in tropical climates and ripen after harvest.");
check(selfMatch > cross, "self-match beats unrelated cross-match", `${selfMatch} > ${cross}`);
check(selfMatch >= 0.75, "self-match clears the 0.75 ratchet", `${selfMatch}`);

const big = "storm ".repeat(9 * 1024); // ~54 KB
const bigScore = score("storm?", "Winds of 12 m/s are expected.", big);
check(Number.isFinite(bigScore) && bigScore >= 0 && bigScore <= 1, "~54 KB repeated text does not trap", `${bigScore}`);

const uni = "\u{1F5FC}中文 café ☃ 23.1°C مرحبا";
const uniScore = score("weather?", "It is 23.1C.", uni);
check(Number.isFinite(uniScore) && uniScore >= 0 && uniScore <= 1, "emoji/CJK/accents do not trap", `${uniScore}`);

const invalidUtf8Score = scoreRawAnswer(Q, GT, Uint8Array.from([0xff, 0xfe, 0x80, 0x61]));
check(
  Number.isFinite(invalidUtf8Score) && invalidUtf8Score >= 0 && invalidUtf8Score <= 1,
  "invalid UTF-8 does not trap",
  `${invalidUtf8Score}`,
);

// The gate makes thousands of calls and never calls dealloc.
let allocOk = true;
for (let i = 0; i < 5000; i++) if (ex.alloc(4096) === 0) { allocOk = false; break; }
check(allocOk, "allocator never returns 0 under sustained load");

console.log("");
console.log("--- Scoring thesis ---");
const cases = [
  ["self-match (ground truth as answer)", GT, "high"],
  ["correct + terse", "The data shows the IP 142.251.42.174 is hosted by Google LLC in the United States.", "high"],
  ["correct as JSON", '{"ip":"142.251.42.174","isp":"Google LLC","country":"United States"}', "high"],
  ["wrong location (fact swap)", "The data shows the IP 142.251.42.174 is hosted by Cloudflare in Mumbai, India.", "low"],
  ["question echo (contentless)", "The data shows the geolocation details for the IP address 142.251.42.174 including the country, city and ISP information.", "low"],
  ["content-filter refusal", "- The generated text has been blocked by our content filters.", "low"],
  ["keyword stuffing", "IP address geolocation country city ISP information lookup details network region provider located hosted", "low"],
  ["off-topic", "Bananas grow in tropical climates and ripen after harvest.", "low"],
  ["empty", "", "zero"],
];
const results = [];
for (const [label, ans, band] of cases) {
  const s = score(Q, GT, ans);
  results.push({ label, s, band });
  console.log(`  ${s.toFixed(6)}  ${label}`);
}

const hi = results.filter((r) => r.band === "high").map((r) => r.s);
const lo = results.filter((r) => r.band === "low").map((r) => r.s);
const minHi = Math.min(...hi);
const maxLo = Math.max(...lo);
console.log("");
check(minHi > maxLo, "every correct answer outranks every wrong/contentless one", `min(good)=${minHi.toFixed(6)} > max(bad)=${maxLo.toFixed(6)}`);

const margin = hi.reduce((a, b) => a + b, 0) / hi.length - lo.reduce((a, b) => a + b, 0) / lo.length;
check(margin >= 0.15, "separation clears the absolute margin floor (0.15)", `margin=${margin.toFixed(6)}`);

const all = results.map((r) => r.s);
const mean = all.reduce((a, b) => a + b, 0) / all.length;
const stddev = Math.sqrt(all.reduce((a, b) => a + (b - mean) ** 2, 0) / all.length);
check(stddev > 0.05, "score spread clears the stddev floor (> 0.05)", `stddev=${stddev.toFixed(6)}`);

console.log("");
console.log("--- Typed facts: right vs wrong figure ---");
const cq = "What is the CVSS score for CVE-2021-44228?";
const cgt = "The CVSS score for CVE-2021-44228 is 10, indicating a critical severity level. Affected versions include Apache Log4j up to 2.14.1.";
const right = score(cq, cgt, "The data shows CVE-2021-44228 has a CVSS score of 10 and is critical in Apache Log4j.");
const near = score(cq, cgt, "The data shows CVE-2021-44228 has a CVSS score of 9.8 and is critical in Apache Log4j.");
const wrong = score(cq, cgt, "The data shows CVE-2021-44228 has a CVSS score of 3.1 and is critical in Apache Log4j.");
console.log(`  CVSS 10  (correct)   -> ${right.toFixed(6)}`);
console.log(`  CVSS 9.8 (near miss) -> ${near.toFixed(6)}`);
console.log(`  CVSS 3.1 (wrong)     -> ${wrong.toFixed(6)}`);
check(right > wrong, "the correct figure outscores the wrong one", `${right.toFixed(6)} > ${wrong.toFixed(6)}`);
check(near > wrong, "a near miss beats a gross miss (continuity, not a cliff)", `${near.toFixed(6)} > ${wrong.toFixed(6)}`);
check(wrong > 0, "a wrong figure degrades rather than falling off a cliff", `${wrong.toFixed(6)}`);

console.log("");
console.log("--- Unit equivalence ---");
const wq = "What wind speed is forecast?";
const wgt = "Sustained winds of 5 m/s are expected overnight.";
const ms = score(wq, wgt, "The data shows sustained winds of 5 m/s overnight.");
const kmh = score(wq, wgt, "The data shows sustained winds of 18 km/h overnight.");
const bad = score(wq, wgt, "The data shows sustained winds of 47 m/s overnight.");
console.log(`  5 m/s   (same unit)       -> ${ms.toFixed(6)}`);
console.log(`  18 km/h (same speed)      -> ${kmh.toFixed(6)}`);
console.log(`  47 m/s  (wrong speed)     -> ${bad.toFixed(6)}`);
check(Math.abs(ms - kmh) < 0.15, "18 km/h scores like 5 m/s (unit normalised)", `|${ms.toFixed(4)} - ${kmh.toFixed(4)}|`);
check(kmh > bad, "the right speed in other units beats a wrong speed", `${kmh.toFixed(6)} > ${bad.toFixed(6)}`);

console.log("");
console.log("--- Determinism ---");
const a1 = score(Q, GT, "The data shows the IP is hosted by Google LLC in the United States.");
score("unrelated", "unrelated ground truth", "unrelated answer");
const a2 = score(Q, GT, "The data shows the IP is hosted by Google LLC in the United States.");
check(Object.is(a1, a2), "no scratch state leaks between calls", `${a1} == ${a2}`);

console.log("");
console.log(failures === 0 ? `ALL CHECKS PASSED (${wasmPath})` : `${failures} CHECK(S) FAILED`);
process.exit(failures === 0 ? 0 : 1);

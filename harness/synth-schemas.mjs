#!/usr/bin/env node

/**
 * Per-intent fact schemas for the synthetic corpus (FIXTURES.md classes 2-10).
 *
 * The honesty rule this file exists to satisfy: synthetic answers are GENERATED
 * from a fact record, never hand-typed while reading a specific ground truth.
 * A schema is written once, blind to any instance:
 *
 *   make(rng)          -> a fact record (the decisive facts of FIXTURES.md's table)
 *   question(f)        -> the question, rendered from the facts
 *   groundTruth(f)     -> the ground truth, rendered from the same facts
 *   surface(f)         -> { lead, parts } the facts as answer clauses
 *   altSurface(f)      -> the same facts in other units / notations (class 7)
 *   temporalWrong(f)   -> right values asserted for the wrong time (class 8)
 *   jsonFacts(f)       -> flat record for the JSON renderings (classes 6, 10)
 *   mutations[]        -> { name, kind, part, apply } one decisive fact changed (classes 2, 5)
 *
 * gen-synth.mjs turns those into answers; no answer text is written against a
 * ground truth a human could see.
 */

export const pick = (rng, list) => list[Math.floor(rng() * list.length)];
export const int = (rng, lo, hi) => lo + Math.floor(rng() * (hi - lo + 1));
export const num = (rng, lo, hi, dp = 1) => Number((lo + rng() * (hi - lo)).toFixed(dp));

const pad = (n) => String(n).padStart(2, "0");
const iso = (d) => `${d.getUTCFullYear()}-${pad(d.getUTCMonth() + 1)}-${pad(d.getUTCDate())}`;
const isoT = (d) => `${iso(d)}T${pad(d.getUTCHours())}:00:00Z`;
export const shiftDays = (isoDate, days) => {
  const d = new Date(`${isoDate}T00:00:00Z`);
  d.setUTCDate(d.getUTCDate() + days);
  return iso(d);
};
const dateAt = (rng, loDay, hiDay) => {
  const d = new Date(Date.UTC(2026, 8, int(rng, loDay, hiDay), int(rng, 0, 23)));
  return { date: iso(d), stamp: isoT(d) };
};
const cToF = (c) => Number((c * 9 / 5 + 32).toFixed(1));
const monthName = (isoDate) =>
  new Date(`${isoDate}T00:00:00Z`).toLocaleDateString("en-US", { timeZone: "UTC", month: "long", day: "numeric", year: "numeric" });
const dms = (v, pos, neg) => `${Math.abs(v).toFixed(4)}° ${v >= 0 ? pos : neg}`;

const CITIES = [
  ["Reykjavik", "Iceland", 64.1466, -21.9426],
  ["Valparaiso", "Chile", -33.0472, -71.6127],
  ["Hobart", "Australia", -42.8821, 147.3272],
  ["Bergen", "Norway", 60.3913, 5.3221],
  ["Windhoek", "Namibia", -22.5609, 17.0658],
  ["Ulaanbaatar", "Mongolia", 47.8864, 106.9057],
];
const CONDITIONS = ["clear skies", "overcast", "moderate rain", "heavy rain", "light drizzle", "scattered thunderstorms"];

export const WEATHER_FORECAST = {
  intent: "WEATHER_FORECAST",
  keywords: ["forecast", "hourly", "temperature", "Celsius", "precipitation probability", "wind speed", "cutoff"],
  make(rng) {
    const [place, country] = pick(rng, CITIES);
    const { date, stamp } = dateAt(rng, 1, 20);
    const lo = num(rng, -8, 18, 1);
    return {
      place, country, start_date: date, cutoff: stamp,
      horizon_hours: pick(rng, [24, 48, 72, 96]),
      temp_min_c: lo, temp_max_c: Number((lo + num(rng, 3, 12, 1)).toFixed(1)),
      precip_prob_pct: int(rng, 0, 100), precip_mm: num(rng, 0, 24, 1),
      wind_ms: num(rng, 0.4, 19, 1), condition: pick(rng, CONDITIONS),
    };
  },
  question: (f) =>
    `Can you provide a ${f.horizon_hours}-hour hourly weather forecast for ${f.place}, ${f.country} starting ` +
    `${f.start_date}, including temperature in Celsius, precipitation probability, and wind speed, and deliver ` +
    `the forecast before the cutoff time of ${f.cutoff}?`,
  groundTruth: (f) =>
    `The ${f.horizon_hours}-hour hourly forecast for ${f.place}, ${f.country} beginning ${f.start_date} shows ` +
    `temperatures ranging from ${f.temp_min_c}°C to ${f.temp_max_c}°C, a maximum precipitation probability ` +
    `of ${f.precip_prob_pct}% with roughly ${f.precip_mm} mm of accumulation, and wind speeds peaking at ` +
    `${f.wind_ms} m/s under ${f.condition}. The forecast is delivered ahead of the ${f.cutoff} cutoff.`,
  surface: (f) => ({
    lead: `${f.horizon_hours}-hour forecast for ${f.place}, ${f.country} starting ${f.start_date}`,
    parts: [
      ["temperature", `temperature ranges from ${f.temp_min_c}°C to ${f.temp_max_c}°C`],
      ["precipitation", `precipitation probability peaks at ${f.precip_prob_pct}% with ${f.precip_mm} mm accumulation`],
      ["wind", `wind speed peaks at ${f.wind_ms} m/s`],
      ["condition", `conditions are ${f.condition}`],
      ["cutoff", `delivered before the ${f.cutoff} cutoff`],
    ],
  }),
  altSurface: (f) => ({
    lead: `${f.horizon_hours}-hour forecast for ${f.place}, ${f.country} starting ${monthName(f.start_date)}`,
    parts: [
      ["temperature", `temperature ranges from ${cToF(f.temp_min_c)}°F to ${cToF(f.temp_max_c)}°F`],
      ["precipitation", `precipitation probability peaks at ${(f.precip_prob_pct / 100).toFixed(2)} on a 0-1 scale with ${(f.precip_mm / 10).toFixed(2)} cm accumulation`],
      ["wind", `wind speed peaks at ${(f.wind_ms * 3.6).toFixed(1)} km/h`],
      ["condition", `conditions are ${f.condition}`],
      ["cutoff", `delivered before the ${f.cutoff} cutoff`],
    ],
  }),
  temporalWrong: (f) => ({
    lead: `${f.horizon_hours}-hour forecast for ${f.place}, ${f.country} starting ${shiftDays(f.start_date, 4)}`,
    parts: [
      ["temperature", `temperature at hour ${f.horizon_hours} is exactly ${f.temp_max_c}°C`],
      ["precipitation", `precipitation probability at hour ${f.horizon_hours} is exactly ${f.precip_prob_pct}%`],
      ["wind", `wind speed at hour ${f.horizon_hours} is exactly ${f.wind_ms} m/s`],
    ],
  }),
  jsonFacts: (f) => ({
    place: `${f.place}, ${f.country}`, start: f.start_date, horizon_hours: f.horizon_hours,
    temp_min_c: f.temp_min_c, temp_max_c: f.temp_max_c, precip_probability_pct: f.precip_prob_pct,
    precip_mm: f.precip_mm, wind_ms: f.wind_ms, condition: f.condition, cutoff: f.cutoff,
  }),
  mutations: [
    { name: "temperature", kind: "numeric", part: "temperature",
      apply: (f, rng) => ({ ...f, temp_min_c: Number((f.temp_min_c + 11 + int(rng, 0, 6)).toFixed(1)), temp_max_c: Number((f.temp_max_c + 11 + int(rng, 0, 6)).toFixed(1)) }) },
    { name: "condition", kind: "categorical", part: "condition",
      apply: (f, rng) => ({ ...f, condition: pick(rng, CONDITIONS.filter((c) => c !== f.condition)), precip_prob_pct: 100 - f.precip_prob_pct }) },
    { name: "start-date", kind: "temporal", part: "cutoff", apply: (f) => ({ ...f, start_date: shiftDays(f.start_date, 9) }) },
    { name: "place", kind: "identifier", part: "temperature",
      apply: (f, rng) => ({ ...f, place: pick(rng, CITIES.filter((c) => c[0] !== f.place))[0] }) },
  ],
};

const VERDICTS = ["valid", "expired", "self-signed", "hostname mismatch", "untrusted root", "unreachable"];
const ISSUERS = ["Let's Encrypt R11", "DigiCert Global G2", "Sectigo RSA DV", "GlobalSign Atlas R3", "ZeroSSL ECC"];
const HOSTS = ["api", "portal", "gateway", "status", "auth", "cdn"];
const ZONES = ["northwind.dev", "contoso.net", "acme-labs.io", "fabrikam.example", "ridgeline.systems"];

export const SSL_VERIFICATION = {
  intent: "SSL_VERIFICATION",
  keywords: ["TLS", "certificate", "issuer", "expiry", "chain", "SAN", "hostname", "verify"],
  make(rng) {
    const host = `${pick(rng, HOSTS)}.${pick(rng, ZONES)}`;
    const { date } = dateAt(rng, 1, 28);
    return {
      hostname: host, verdict: pick(rng, VERDICTS), issuer: pick(rng, ISSUERS),
      expiry: shiftDays(date, int(rng, -120, 400)), days_remaining: int(rng, -120, 400),
      hostname_match: rng() > 0.3, chain_complete: rng() > 0.3,
      sans: [host, `www.${host.split(".").slice(1).join(".")}`], serial_tail: int(rng, 1000, 9999),
    };
  },
  question: (f) =>
    `Verify the TLS certificate presented by ${f.hostname}: is it currently valid, which certificate authority ` +
    `issued it, on what date does it expire, does the hostname match the subject alternative names, and is the ` +
    `certificate chain complete?`,
  groundTruth: (f) =>
    `The TLS certificate presented by ${f.hostname} is ${f.verdict}. It was issued by ${f.issuer}, expires on ` +
    `${f.expiry} (${f.days_remaining} days from the check), and the hostname ` +
    `${f.hostname_match ? "matches" : "does not match"} the subject alternative names ${f.sans.join(", ")}. The ` +
    `certificate chain is ${f.chain_complete ? "complete" : "incomplete"}; serial ends ${f.serial_tail}.`,
  surface: (f) => ({
    lead: `TLS certificate for ${f.hostname}`,
    parts: [
      ["verdict", `the certificate is ${f.verdict}`],
      ["expiry", `it expires on ${f.expiry}, ${f.days_remaining} days from the check`],
      ["issuer", `it was issued by ${f.issuer}`],
      ["hostname", `the hostname ${f.hostname_match ? "matches" : "does not match"} the SANs ${f.sans.join(", ")}`],
      ["chain", `the chain is ${f.chain_complete ? "complete" : "incomplete"}`],
    ],
  }),
  altSurface: (f) => ({
    lead: `TLS certificate for ${f.hostname}`,
    parts: [
      ["verdict", `certificate status: ${f.verdict.toUpperCase()}`],
      ["expiry", `not-after ${monthName(f.expiry)} (${f.days_remaining} d remaining)`],
      ["issuer", `CA: ${f.issuer}`],
      ["hostname", `SAN match: ${f.hostname_match ? "yes" : "no"} over ${f.sans.join(" / ")}`],
      ["chain", `chain completeness: ${f.chain_complete ? "1/1" : "0/1"}`],
    ],
  }),
  temporalWrong: (f) => ({
    lead: `TLS certificate for ${f.hostname}`,
    parts: [
      ["verdict", `the certificate is ${f.verdict}`],
      ["expiry", `${f.expiry} is the date the certificate was ISSUED, not the date it expires`],
      ["issuer", `it was issued by ${f.issuer}`],
    ],
  }),
  jsonFacts: (f) => ({
    hostname: f.hostname, verdict: f.verdict, not_after: f.expiry, days_remaining: f.days_remaining,
    issuer: f.issuer, hostname_match: f.hostname_match, chain_complete: f.chain_complete, sans: f.sans,
  }),
  mutations: [
    { name: "expiry", kind: "temporal", part: "expiry",
      apply: (f) => ({ ...f, expiry: shiftDays(f.expiry, 213), days_remaining: f.days_remaining + 213 }) },
    { name: "verdict", kind: "categorical", part: "verdict",
      apply: (f, rng) => ({ ...f, verdict: pick(rng, VERDICTS.filter((v) => v !== f.verdict)) }) },
    { name: "issuer", kind: "identifier", part: "issuer",
      apply: (f, rng) => ({ ...f, issuer: pick(rng, ISSUERS.filter((i) => i !== f.issuer)) }) },
    { name: "days-remaining", kind: "numeric", part: "expiry",
      apply: (f, rng) => ({ ...f, days_remaining: f.days_remaining + 90 + int(rng, 0, 60) }) },
  ],
};

const SEVERITIES = ["LOW", "MEDIUM", "HIGH", "CRITICAL"];
const PRODUCTS = ["libmesh", "corvid-router", "atlas-cache", "quill-parser", "vellum-tls"];

export const CVE_LOOKUP = {
  intent: "CVE_LOOKUP",
  keywords: ["CVE", "vulnerability", "CVSS", "severity", "affected versions", "patched", "advisory"],
  make(rng) {
    const sev = pick(rng, SEVERITIES);
    const base = { LOW: [0.1, 3.9], MEDIUM: [4, 6.9], HIGH: [7, 8.9], CRITICAL: [9, 10] }[sev];
    const major = int(rng, 1, 6);
    return {
      cve_id: `CVE-20${int(rng, 22, 26)}-${int(rng, 10000, 49999)}`,
      product: pick(rng, PRODUCTS), severity: sev, cvss: num(rng, base[0], base[1], 1),
      affected_from: `${major}.0.0`, affected_to: `${major}.${int(rng, 1, 8)}.${int(rng, 0, 9)}`,
      fixed_in: `${major}.${int(rng, 9, 12)}.0`, vector: pick(rng, ["network", "local", "adjacent"]),
      published: dateAt(rng, 1, 27).date,
    };
  },
  question: (f) =>
    `Look up ${f.cve_id} affecting ${f.product}: what is its severity rating, what CVSS base score was assigned, ` +
    `which versions are affected, and in which release was it fixed?`,
  groundTruth: (f) =>
    `${f.cve_id} affects ${f.product} and is rated ${f.severity} with a CVSS base score of ${f.cvss}. Versions ` +
    `${f.affected_from} through ${f.affected_to} are affected over a ${f.vector} attack vector; the issue is fixed ` +
    `in ${f.fixed_in}. The advisory was published on ${f.published}.`,
  surface: (f) => ({
    lead: `${f.cve_id} in ${f.product}`,
    parts: [
      ["severity", `severity is ${f.severity}`],
      ["cvss", `the CVSS base score is ${f.cvss}`],
      ["versions", `versions ${f.affected_from} through ${f.affected_to} are affected`],
      ["fixed", `it is fixed in ${f.fixed_in}`],
      ["vector", `the attack vector is ${f.vector}`],
    ],
  }),
  altSurface: (f) => ({
    lead: `${f.cve_id} in ${f.product}`,
    parts: [
      ["severity", `severity band: ${f.severity.toLowerCase()}`],
      ["cvss", `CVSS v3.1 base ${f.cvss.toFixed(1)}/10 (${(f.cvss / 10).toFixed(2)} normalised)`],
      ["versions", `affected range >=${f.affected_from}, <=${f.affected_to}`],
      ["fixed", `first fixed release ${f.fixed_in}`],
      ["vector", `AV:${f.vector[0].toUpperCase()} (${f.vector})`],
    ],
  }),
  temporalWrong: (f) => ({
    lead: `${f.cve_id} in ${f.product}`,
    parts: [
      ["severity", `severity is ${f.severity}`],
      ["cvss", `the CVSS base score is ${f.cvss}`],
      ["versions", `${f.fixed_in} is an affected version and ${f.affected_to} is the fixed release`],
    ],
  }),
  jsonFacts: (f) => ({
    cve: f.cve_id, product: f.product, severity: f.severity, cvss_base: f.cvss,
    affected: `${f.affected_from} - ${f.affected_to}`, fixed_in: f.fixed_in, vector: f.vector, published: f.published,
  }),
  mutations: [
    { name: "cvss", kind: "numeric", part: "cvss", apply: (f) => ({ ...f, cvss: Number(Math.min(10, Math.max(0.1, f.cvss > 5 ? f.cvss - 4.2 : f.cvss + 4.2)).toFixed(1)) }) },
    { name: "severity", kind: "categorical", part: "severity",
      apply: (f, rng) => ({ ...f, severity: pick(rng, SEVERITIES.filter((s) => s !== f.severity)) }) },
    { name: "cve-id", kind: "identifier", part: "severity",
      apply: (f, rng) => ({ ...f, cve_id: `CVE-20${int(rng, 22, 26)}-${int(rng, 50000, 89999)}` }) },
    { name: "fixed-version", kind: "temporal", part: "fixed",
      apply: (f, rng) => ({ ...f, fixed_in: `${int(rng, 7, 12)}.${int(rng, 0, 9)}.0` }) },
  ],
};

const REGIONS = [
  ["Iceland", "Capital Region", "Reykjavik", 64.1466, -21.9426, "Ljosleidarinn ehf"],
  ["Kenya", "Nairobi County", "Nairobi", -1.2921, 36.8219, "Safaricom PLC"],
  ["Portugal", "Norte", "Porto", 41.1579, -8.6291, "NOS Comunicacoes"],
  ["Vietnam", "Da Nang", "Da Nang", 16.0544, 108.2022, "Viettel Group"],
  ["Uruguay", "Montevideo", "Montevideo", -34.9011, -56.1645, "Administracion Nacional"],
];

export const IP_GEOLOCATION = {
  intent: "IP_GEOLOCATION",
  keywords: ["IP address", "geolocation", "country", "region", "city", "ISP", "coordinates", "ASN"],
  make(rng) {
    const [country, region, city, lat, lon, isp] = pick(rng, REGIONS);
    return {
      ip: `${int(rng, 12, 210)}.${int(rng, 0, 255)}.${int(rng, 0, 255)}.${int(rng, 1, 254)}`,
      country, region, city, lat, lon, isp, asn: `AS${int(rng, 1000, 65000)}`,
    };
  },
  question: (f) =>
    `Geolocate the IP address ${f.ip}: which country, region and city does it resolve to, which ISP or ` +
    `organisation announces it, and what are its approximate coordinates?`,
  groundTruth: (f) =>
    `The IP address ${f.ip} resolves to ${f.city}, ${f.region}, ${f.country}. It is announced by ${f.isp} ` +
    `(${f.asn}). Approximate coordinates are ${f.lat}, ${f.lon}.`,
  surface: (f) => ({
    lead: `IP address ${f.ip}`,
    parts: [
      ["country", `it resolves to ${f.country}`],
      ["place", `the region is ${f.region} and the city is ${f.city}`],
      ["isp", `the announcing organisation is ${f.isp} (${f.asn})`],
      ["coords", `coordinates are ${f.lat}, ${f.lon}`],
    ],
  }),
  altSurface: (f) => ({
    lead: `IP address ${f.ip}`,
    parts: [
      ["country", `country: ${f.country}`],
      ["place", `${f.city} / ${f.region}`],
      ["isp", `ASN ${f.asn.slice(2)} operated by ${f.isp}`],
      ["coords", `coordinates ${dms(f.lat, "N", "S")}, ${dms(f.lon, "E", "W")}`],
    ],
  }),
  temporalWrong: (f) => ({
    lead: `IP address ${f.ip}`,
    parts: [
      ["country", `it resolved to ${f.country} in a 2019 snapshot; the current allocation was not checked`],
      ["place", `the region was ${f.region} and the city was ${f.city} at that time`],
    ],
  }),
  jsonFacts: (f) => ({ ip: f.ip, country: f.country, region: f.region, city: f.city, isp: f.isp, asn: f.asn, lat: f.lat, lon: f.lon }),
  mutations: [
    { name: "coordinates", kind: "numeric", part: "coords",
      apply: (f) => ({ ...f, lat: Number((f.lat + 23.5).toFixed(4)), lon: Number((f.lon - 41.25).toFixed(4)) }) },
    { name: "country", kind: "categorical", part: "country",
      apply: (f, rng) => { const r = pick(rng, REGIONS.filter((x) => x[0] !== f.country)); return { ...f, country: r[0], region: r[1], city: r[2] }; } },
    { name: "isp", kind: "identifier", part: "isp",
      apply: (f, rng) => ({ ...f, isp: pick(rng, REGIONS.filter((x) => x[5] !== f.isp))[5], asn: `AS${int(rng, 1000, 65000)}` }) },
    { name: "ip", kind: "identifier", part: "country",
      apply: (f, rng) => ({ ...f, ip: `${int(rng, 12, 210)}.${int(rng, 0, 255)}.${int(rng, 0, 255)}.${int(rng, 1, 254)}` }) },
  ],
};

function priceSchema(intent, keywords, symbols, quote, build) {
  return {
    intent, keywords,
    make(rng) {
      const [symbol, name, lo, hi] = pick(rng, symbols);
      const { stamp } = dateAt(rng, 1, 27);
      return { symbol, name, price: num(rng, lo, hi, 2), currency: "USD", as_of: stamp, change_pct: num(rng, -8, 8, 2), venue: quote(rng) };
    },
    question: (f) => build.question(f),
    groundTruth: (f) =>
      `${f.name} (${f.symbol}) is quoted at ${f.price} ${f.currency} as of ${f.as_of} on ${f.venue}, a change of ` +
      `${f.change_pct}% over the prior close.`,
    surface: (f) => ({
      lead: `${f.name} (${f.symbol})`,
      parts: [
        ["price", `the price is ${f.price} ${f.currency}`],
        ["asof", `as of ${f.as_of}`],
        ["venue", `sourced from ${f.venue}`],
        ["change", `the change against the prior close is ${f.change_pct}%`],
      ],
    }),
    altSurface: (f) => ({
      lead: `${f.name} (${f.symbol})`,
      parts: [
        ["price", `the price is $${f.price} (${(f.price / 1000).toFixed(5)} thousand ${f.currency})`],
        ["asof", `as of ${monthName(f.as_of.slice(0, 10))} ${f.as_of.slice(11, 16)} UTC`],
        ["venue", `source: ${f.venue}`],
        ["change", `change of ${(f.change_pct / 100).toFixed(4)} on a fractional scale`],
      ],
    }),
    temporalWrong: (f) => ({
      lead: `${f.name} (${f.symbol})`,
      parts: [
        ["price", `the price is ${f.price} ${f.currency}`],
        ["asof", `that is the 30-day high, not the quote as of ${f.as_of}`],
      ],
    }),
    jsonFacts: (f) => ({ symbol: f.symbol, name: f.name, price: f.price, currency: f.currency, as_of: f.as_of, venue: f.venue, change_pct: f.change_pct }),
    mutations: [
      { name: "price", kind: "numeric", part: "price", apply: (f) => ({ ...f, price: Number((f.price * 1.62 + 7).toFixed(2)) }) },
      { name: "direction", kind: "categorical", part: "change", apply: (f) => ({ ...f, change_pct: Number((-f.change_pct).toFixed(2)) }) },
      { name: "symbol", kind: "identifier", part: "price",
        apply: (f, rng) => { const s = pick(rng, symbols.filter((x) => x[0] !== f.symbol)); return { ...f, symbol: s[0], name: s[1] }; } },
      { name: "as-of", kind: "temporal", part: "asof", apply: (f) => ({ ...f, as_of: `${shiftDays(f.as_of.slice(0, 10), -17)}T${f.as_of.slice(11)}` }) },
    ],
  };
}

export const CRYPTO_PRICE = priceSchema(
  "CRYPTO_PRICE",
  ["crypto", "price", "USD", "pair", "spot", "as of", "exchange"],
  [["BTC/USD", "Bitcoin", 20000, 120000], ["ETH/USD", "Ether", 900, 6000], ["SOL/USD", "Solana", 20, 400], ["XMR/USD", "Monero", 90, 500]],
  (rng) => pick(rng, ["Coinbase", "Kraken", "Binance", "Bitstamp"]),
  { question: (f) => `What is the current spot price of ${f.name} (${f.symbol}) in USD as of ${f.as_of}, on which venue was it quoted, and how has it moved against the prior close?` },
);

export const STOCK_PRICE = priceSchema(
  "STOCK_PRICE",
  ["stock", "share price", "ticker", "USD", "close", "exchange", "as of"],
  [["NVDA", "NVIDIA Corporation", 60, 900], ["ASML", "ASML Holding", 400, 1200], ["TSM", "Taiwan Semiconductor", 60, 300], ["NOVO", "Novo Nordisk", 40, 200]],
  (rng) => pick(rng, ["NASDAQ", "NYSE", "Euronext Amsterdam", "Nasdaq Copenhagen"]),
  { question: (f) => `What is the share price of ${f.name} (${f.symbol}) in USD as of ${f.as_of}, on which exchange does it trade, and how has it moved against the prior close?` },
);

export const STORM_ALERT = {
  intent: "STORM_ALERT",
  keywords: ["storm", "alert", "wind", "gusts", "precipitation", "risk", "coordinates", "valid"],
  make(rng) {
    const [place, country, lat, lon] = pick(rng, CITIES);
    const { stamp } = dateAt(rng, 1, 20);
    const wind = num(rng, 8, 130, 1);
    return {
      place, country, lat, lon, valid_at: stamp, window_hours: pick(rng, [6, 12, 24, 48]),
      wind_kmh: wind, gust_kmh: Number((wind * num(rng, 1.2, 1.9, 2)).toFixed(1)),
      precip_mm: num(rng, 0, 90, 1), risk: num(rng, 0, 1, 2),
      time_mode: pick(rng, ["point", "window"]),
    };
  },
  question: (f) =>
    `Is there a storm alert for ${f.place}, ${f.country} at ${f.lat}, ${f.lon} valid at ${f.valid_at} over the next ` +
    `${f.window_hours} hours? Report sustained wind speed, peak gusts, expected precipitation, and a risk value ` +
    `between 0 and 1.`,
  groundTruth: (f) =>
    `For ${f.place}, ${f.country} at ${f.lat}, ${f.lon}, valid at ${f.valid_at} over the next ${f.window_hours} ` +
    `hours, sustained wind reaches ${f.wind_kmh} km/h with peak gusts of ${f.gust_kmh} km/h, precipitation totals ` +
    `${f.precip_mm} mm, and the storm risk is ${f.risk} on a 0 to 1 scale. This is a ${f.time_mode} value.`,
  surface: (f) => ({
    lead: `storm alert for ${f.place}, ${f.country} at ${f.lat}, ${f.lon} valid at ${f.valid_at}`,
    parts: [
      ["wind", `sustained wind reaches ${f.wind_kmh} km/h`],
      ["gust", `peak gusts reach ${f.gust_kmh} km/h`],
      ["precip", `precipitation totals ${f.precip_mm} mm`],
      ["risk", `the risk value is ${f.risk} on a 0 to 1 scale`],
      ["window", `over the next ${f.window_hours} hours as a ${f.time_mode} value`],
    ],
  }),
  altSurface: (f) => ({
    lead: `storm alert for ${f.place}, ${f.country} at ${dms(f.lat, "N", "S")}, ${dms(f.lon, "E", "W")} valid at ${f.valid_at}`,
    parts: [
      ["wind", `sustained wind reaches ${(f.wind_kmh / 3.6).toFixed(1)} m/s`],
      ["gust", `peak gusts reach ${(f.gust_kmh / 3.6).toFixed(1)} m/s`],
      ["precip", `precipitation totals ${(f.precip_mm / 10).toFixed(2)} cm`],
      ["risk", `the risk value is ${(f.risk * 100).toFixed(0)}%`],
      ["window", `over the next ${f.window_hours} hours as a ${f.time_mode} value`],
    ],
  }),
  temporalWrong: (f) => ({
    lead: `storm alert for ${f.place}, ${f.country} at ${f.lat}, ${f.lon}`,
    parts: [
      ["wind", `${f.wind_kmh} km/h is the maximum across the whole ${f.window_hours}-hour window, reported here as the instantaneous value at ${f.valid_at}`],
      ["gust", `peak gusts reach ${f.gust_kmh} km/h`],
      ["risk", `the risk value is ${f.risk} on a 0 to 1 scale`],
    ],
  }),
  jsonFacts: (f) => ({
    place: `${f.place}, ${f.country}`, lat: f.lat, lon: f.lon, valid_at: f.valid_at, window_hours: f.window_hours,
    wind_kmh: f.wind_kmh, gust_kmh: f.gust_kmh, precip_mm: f.precip_mm, risk: f.risk, time_mode: f.time_mode,
  }),
  mutations: [
    { name: "wind", kind: "numeric", part: "wind",
      apply: (f) => ({ ...f, wind_kmh: Number((f.wind_kmh * 2.4 + 15).toFixed(1)), gust_kmh: Number((f.gust_kmh * 2.4 + 15).toFixed(1)) }) },
    { name: "risk", kind: "numeric", part: "risk", apply: (f) => ({ ...f, risk: Number(Math.abs(1 - f.risk - 0.03).toFixed(2)) }) },
    { name: "time-mode", kind: "categorical", part: "window",
      apply: (f) => ({ ...f, time_mode: f.time_mode === "point" ? "window" : "point", window_hours: f.window_hours === 6 ? 48 : 6 }) },
    { name: "coordinates", kind: "identifier", part: "wind",
      apply: (f) => ({ ...f, lat: Number((f.lat + 18.75).toFixed(4)), lon: Number((f.lon - 33.5).toFixed(4)) }) },
  ],
};

const VENUES = ["NeurIPS", "ICML", "ACL", "CVPR", "ICLR", "EMNLP", "SIGGRAPH", "VLDB"];
const FIELDS = ["protein folding", "sparse attention", "differential privacy", "graph neural networks",
  "speech separation", "causal inference", "federated learning", "program synthesis"];
const SURNAMES = ["Okonkwo", "Lindqvist", "Ramaswamy", "Beaulieu", "Nakagawa", "Ferreira", "Haddad", "Novakova"];

/**
 * ACADEMIC_SEARCH. The champion for this intent orders only 9 of 13 hidden
 * pairs, so ordering -- not separation -- is the reachable gate. The mutations
 * below are therefore weighted toward the distinctions a citation-shaped answer
 * must get right: which year, which venue, whose paper, and how many citations.
 */
export const ACADEMIC_SEARCH = {
  intent: "ACADEMIC_SEARCH",
  keywords: ["paper", "authors", "published", "venue", "citations", "abstract", "DOI", "study"],
  make(rng) {
    const first = pick(rng, SURNAMES);
    let second = pick(rng, SURNAMES);
    while (second === first) second = pick(rng, SURNAMES);
    const year = int(rng, 2018, 2026);
    return {
      field: pick(rng, FIELDS),
      lead: first,
      coauthor: second,
      year,
      venue: pick(rng, VENUES),
      citations: int(rng, 40, 4200),
      doi: `10.${int(rng, 1000, 9999)}/${pick(rng, VENUES).toLowerCase()}.${year}.${int(rng, 100, 999)}`,
      pages: `${int(rng, 1, 400)}-${int(rng, 401, 800)}`,
    };
  },
  question: (f) =>
    `Find the most cited paper on ${f.field} led by ${f.lead}: who are the authors, in which year and venue ` +
    `was it published, and how many citations does it have?`,
  groundTruth: (f) =>
    `The paper on ${f.field} was authored by ${f.lead} and ${f.coauthor} and published at ${f.venue} in ` +
    `${f.year}. It has ${f.citations} citations and appears at pages ${f.pages} under DOI ${f.doi}.`,
  surface: (f) => ({
    lead: `the ${f.field} paper led by ${f.lead}`,
    parts: [
      ["authors", `the authors are ${f.lead} and ${f.coauthor}`],
      ["year", `it was published in ${f.year}`],
      ["venue", `it appeared at ${f.venue}`],
      ["citations", `it has ${f.citations} citations`],
      ["doi", `the DOI is ${f.doi}`],
    ],
  }),
  altSurface: (f) => ({
    lead: `${f.lead} et al. on ${f.field}`,
    parts: [
      ["authors", `authorship: ${f.lead}, ${f.coauthor}`],
      ["year", `year of publication ${f.year}`],
      ["venue", `venue ${f.venue} (proceedings)`],
      ["citations", `cited ${f.citations} times to date`],
      ["doi", `doi:${f.doi}`],
    ],
  }),
  temporalWrong: (f) => ({
    lead: `the ${f.field} paper led by ${f.lead}`,
    parts: [
      ["authors", `the authors are ${f.lead} and ${f.coauthor}`],
      ["year", `it was published in ${f.year - 4}, and the ${f.year} entry is a later reprint`],
      ["citations", `it has ${f.citations} citations`],
    ],
  }),
  jsonFacts: (f) => ({
    field: f.field, authors: [f.lead, f.coauthor], year: f.year, venue: f.venue,
    citations: f.citations, doi: f.doi, pages: f.pages,
  }),
  mutations: [
    { name: "citations", kind: "numeric", part: "citations",
      apply: (f) => ({ ...f, citations: f.citations > 2000 ? f.citations - 1900 : f.citations + 1900 }) },
    { name: "venue", kind: "categorical", part: "venue",
      apply: (f, rng) => ({ ...f, venue: pick(rng, VENUES.filter((v) => v !== f.venue)) }) },
    { name: "coauthor", kind: "identifier", part: "authors",
      apply: (f, rng) => ({ ...f, coauthor: pick(rng, SURNAMES.filter((n) => n !== f.lead && n !== f.coauthor)) }) },
    { name: "year", kind: "temporal", part: "year",
      apply: (f, rng) => ({ ...f, year: f.year - int(rng, 3, 6) }) },
    { name: "doi", kind: "identifier", part: "doi",
      apply: (f, rng) => ({ ...f, doi: `10.${int(rng, 1000, 9999)}/misc.${int(rng, 100, 999)}` }) },
  ],
};

export const SCHEMAS = {
  ACADEMIC_SEARCH,
  WEATHER_FORECAST, SSL_VERIFICATION, STORM_ALERT, CVE_LOOKUP, IP_GEOLOCATION, CRYPTO_PRICE, STOCK_PRICE,
};

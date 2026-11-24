// Compares js/v8-json.js (V8's JSON.parse error messages, written out for QuickJS) with the
// JSON.parse of the V8 this Node runs on, over hand-picked and generated inputs.
//
//   node v8-json.mjs [rounds]
//
// It prints the inputs whose messages differ and exits 1 when there are any. With --write it also
// writes v8-json.json: a sample of inputs with V8's messages, which tests/golden.rs replays in
// QuickJS.
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const source = readFileSync(join(here, "..", "..", "js", "v8-json.js"), "utf8");
const [v8JsonParseError, v8JsonCycleMessage] = new Function(source + "\nreturn [__v8JsonParseError, __v8JsonCycleMessage];")();

const v8 = (text) => {
  try {
    JSON.parse(text);
    return null;
  } catch (e) {
    return e.message;
  }
};

const picked = [
  "", " ", "not json", "{'a': 1}", "undefined", "NaN", "Infinity", "-Infinity", "[object Object]", " [object Object]", '{"a": 1,}', '{"a": [1, 2',
  "<!DOCTYPE html><html></html>", "{a: 1}", '"\\x41"', '"line\nbreak"', "Sorry, I could not find any results for your query. Please try again later.",
  '{"code": "async () => 1', '{"code": "async () => 1",}', '{"code": "async () => 1"}{"code": "async () => 2"}', "<code>async () => 1</code>", "async () => 1",
  "01", "-", "-x", "1.", "1.x", "1e", "1e+", "1E-x", "-0", "-01", "0x10", "00", "0.", "0e", "0-", "1-", "+1", ".5", "1..2", "--1", "1e5", "1 2", "12abc",
  "t", "tr", "tru", "true", "truE", "trux", "truee", "f", "fals", "falsy", "n", "nul", "nulL", "nul ", "nullx", "null null",
  '"', '"abc', '"abc\\', '"\\u12', '"\\u12G4"', '"\\uZZZZ"', '"\\☃"', '"\\é"', '"tab\there"', '"\\', '"\\u', '"\u0000"', '"ok" x', '"ok"x',
  "[", "[1", "[1,", "[1,]", "[,1]", "[1 2]", "[1;2]", "]", "[]]", "[[]", "[1,,2]", '["a" "b"]', "[1}",
  "{", '{"', '{"a', '{"a"', '{"a":', '{"a":1', '{"a":1,', '{"a":1,"', '{"a":1 "b":2}', '{"a" 1}', '{"a":}', "{1:2}", "{,}", '{"a":1,,}', "{]", '{"a":1]', "{}}", '{"a":1}}', "{} {}",
  ":", ",", "}", "?", "é", "☃", "😀", "\ud83d", "abc\ud83ddef0123456789012345", "x".repeat(30), " ".repeat(25) + "x", "0123456789" + "x" + "0123456789",
  "0123456789012345678x", "01234567890123456789x", "012345678901234567890x", "[" + "1,".repeat(12) + "x]", "\n\n  {\n  \"a\": tru\n}", "[1,\r\n2,\r\nx]", "[1,\r2,\nx]", "{\"a\":\r\n\r\n\"b\" x}",
  "'single'", "`template`", "/* c */ 1", "// c\n1", "{\"a\": undefined}", "{\"a\": NaN}", "[undefined]", "[NaN]", "True", "None", "nil", "yes", "﻿{}", "{} ", " {}", "\t\n\r 1 \t\n\r", "1 ",
];

// A small deterministic generator: valid documents, cut and damaged.
let seed = 12345;
const rand = (n) => {
  // mulberry32
  seed = (seed + 0x6d2b79f5) | 0;
  let t = Math.imul(seed ^ (seed >>> 15), 1 | seed);
  t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
  return ((t ^ (t >>> 14)) >>> 0) % n;
};
const pieces = ['{"a":1}', "[1,2,3]", '"str"', "true", "false", "null", "-12.5e3", "0", '{"k":[{"x":null},"y\\n"]}', '"\\u00e9\\t"', "[[],{}]", '{"long key name":"long string value here"}'];
const junk = ['"', "\\", "{", "}", "[", "]", ":", ",", " ", "\n", "\r\n", "x", "é", "☃", "-", "0", "1", ".", "e", "t", "n", "f", "u", "\t", "\u0001", "'", "+", "/"];
function generated() {
  let doc = pieces[rand(pieces.length)];
  const depth = rand(3);
  for (let d = 0; d < depth; d++) doc = rand(2) ? `[${doc},${pieces[rand(pieces.length)]}]` : `{"p${d}":${doc},"q":${pieces[rand(pieces.length)]}}`;
  if (rand(3) === 0) doc = " ".repeat(rand(12)) + doc + "\n".repeat(rand(3));
  const edits = 1 + rand(3);
  for (let i = 0; i < edits; i++) {
    const at = rand(doc.length + 1);
    switch (rand(4)) {
      case 0:
        doc = doc.slice(0, at);
        break;
      case 1:
        doc = doc.slice(0, at) + junk[rand(junk.length)] + doc.slice(at);
        break;
      case 2:
        doc = doc.slice(0, at) + doc.slice(at + 1 + rand(3));
        break;
      default:
        doc = doc.slice(0, at) + junk[rand(junk.length)] + doc.slice(at + 1);
    }
  }
  return doc;
}

const rounds = Number(process.argv.find((a) => /^\d+$/.test(a)) ?? 200000);
let differ = 0;
const sample = [];
const seen = new Set();
const compare = (text, keep) => {
  const expected = v8(text);
  const got = v8JsonParseError(text);
  if (expected !== got) {
    differ++;
    if (differ <= 40) console.log(JSON.stringify({ text, v8: expected, port: got }));
  }
  // The sample is replayed from Rust, whose strings cannot hold a lone surrogate.
  if (keep && !seen.has(text) && text.isWellFormed() && (expected ?? "").isWellFormed()) {
    seen.add(text);
    sample.push([text, expected]);
  }
};
for (const text of picked) compare(text, true);
for (let i = 0; i < rounds; i++) compare(generated(), i < 400);
console.log(`${picked.length + rounds} inputs, ${differ} differ (V8 ${process.versions.v8})`);

// Values that contain themselves: JSON.stringify's message, which shows the path of the cycle.
class Node {
  constructor(name) {
    this.name = name;
  }
}
function cyclic() {
  const make = () => (rand(4) === 0 ? [] : rand(5) === 0 ? new Node("n") : rand(9) === 0 ? Object.create(null) : {});
  const nodes = Array.from({ length: 2 + rand(7) }, make);
  const link = (from, to) => {
    if (Array.isArray(from)) from.push(to);
    else from[["a", "b", "child", "next", "it's", "", "0", "long key"][rand(8)] + (rand(3) ? "" : rand(4))] = to;
  };
  for (let i = 0; i < nodes.length - 1; i++) {
    if (rand(3) === 0) link(nodes[i], rand(2) ? "leaf" : { leaf: true });
    link(nodes[i], nodes[i + 1]);
  }
  link(nodes[nodes.length - 1], nodes[rand(nodes.length)]);
  if (rand(4) === 0) return { wrapper: [1, nodes[0]] };
  return nodes[0];
}
let cycleDiffer = 0;
const cycles = [];
const cycleRounds = Math.min(rounds, 20000);
for (let i = 0; i < cycleRounds; i++) {
  const value = cyclic();
  let expected = null;
  try {
    JSON.stringify(value);
  } catch (e) {
    expected = e.message;
  }
  const got = v8JsonCycleMessage(value);
  if (expected !== got) {
    cycleDiffer++;
    if (cycleDiffer <= 10) console.log(JSON.stringify({ v8: expected, port: got }));
  }
}
console.log(`${cycleRounds} cyclic values, ${cycleDiffer} differ`);
differ += cycleDiffer;
if (process.argv.includes("--write")) {
  writeFileSync(join(here, "v8-json.json"), JSON.stringify({ v8: process.versions.v8, cases: sample }, null, 0) + "\n");
  console.log(`wrote v8-json.json (${sample.length} cases)`);
}
process.exit(differ ? 1 : 0);

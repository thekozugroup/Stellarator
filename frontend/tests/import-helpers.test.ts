// The import screen's pure helpers: dataset names, the default split and Simple mode's summary.
// Run: npm test (Node 22.6 or later; types are stripped, not checked).
import { test } from "node:test"
import assert from "node:assert/strict"
import {
  defaultSplit,
  importSummary,
  isDatasetId,
  parseDataset,
  type SummaryPreview,
  type SummaryRow,
  type SummaryState,
} from "../src/components/import/helpers.ts"

/** "Term: value", with names from the data in backticks. */
function lines(rows: SummaryRow[]): string[] {
  return rows.map((r) => `${r.term}: ${r.parts.map((p) => (typeof p === "string" ? p : `\`${p.code}\``)).join("")}${r.warn ? " (warn)" : ""}`)
}

const col = (name: string, kind: "text" | "messages" | "number" | "other" = "text", looks_like_output = false) => ({
  name,
  kind,
  example: null,
  looks_like_output,
})

const base: SummaryPreview = {
  columns: [],
  prompt_field: null,
  system_field: null,
  split_mode: null,
  has_conversations: false,
  dropped: [],
  hf_splits: [],
  hf_config: null,
  hf_split: null,
  prompts: [],
}

const recommended: SummaryState = {
  source: "huggingface",
  turns: "first_user",
  allSplits: false,
  format: "auto",
  offset: null,
  limit: null,
  dedupe: true,
  name: null,
}

test("links and names load as owner/name; partial input does not", () => {
  for (const [input, id] of [
    ["openai/gsm8k", "openai/gsm8k"],
    ["https://huggingface.co/datasets/openai/gsm8k", "openai/gsm8k"],
    ["https://huggingface.co/datasets/openai/gsm8k/viewer/main/train", "openai/gsm8k"],
    ["huggingface.co/datasets/HuggingFaceH4/ultrachat_200k?row=3", "HuggingFaceH4/ultrachat_200k"],
  ]) {
    assert.equal(parseDataset(input), id)
    assert.ok(isDatasetId(parseDataset(input)), input)
  }
  for (const input of ["", "openai", "openai/", "https://huggingface.co/datasets/", "open ai/gsm8k", "squad"]) {
    assert.ok(!isDatasetId(parseDataset(input)), input)
  }
})

test("the default split is train, then a train-like name, then the first", () => {
  assert.equal(defaultSplit(["test", "train"]), "train")
  assert.equal(defaultSplit(["test_sft", "train_sft", "test_gen", "train_gen"]), "train_sft")
  assert.equal(defaultSplit(["train_sft", "train"]), "train")
  assert.equal(defaultSplit(["validation", "test"]), "validation")
  assert.equal(defaultSplit([]), undefined)
})

test("a question/answer dataset: the prompt column, its split and config, no system prompt", () => {
  const p: SummaryPreview = {
    ...base,
    columns: [col("question"), col("answer", "text", true)],
    prompt_field: "question",
    hf_splits: [
      { config: "main", split: "train", num_rows: 7473 },
      { config: "main", split: "test", num_rows: 1319 },
      { config: "socratic", split: "train", num_rows: 7473 },
    ],
    hf_config: "main",
    hf_split: "train",
    prompts: [[{ role: "user", content: "Natalia sold clips…" }]],
  }
  assert.deepEqual(lines(importSummary(p, recommended)), [
    "Prompts: `question` column",
    "Config: `main`",
    "Split: `train`",
    "System prompt: None",
  ])
})

test("a conversation dataset: the turns setting, a one-config split, and what is left out", () => {
  const p: SummaryPreview = {
    ...base,
    columns: [col("prompt"), col("messages", "messages"), col("prompt_id")],
    prompt_field: "messages",
    has_conversations: true,
    dropped: ["assistant turns in messages"],
    hf_splits: [
      { config: "default", split: "train_sft", num_rows: 207865 },
      { config: "default", split: "test_sft", num_rows: 23110 },
    ],
    hf_config: "default",
    hf_split: "train_sft",
    prompts: [[{ role: "user", content: "Hi" }]],
  }
  assert.deepEqual(lines(importSummary(p, recommended)), [
    "Prompts: `messages`, first user turn",
    "Split: `train_sft`",
    "System prompt: None",
    "Left out: assistant turns in `messages`",
  ])
  // Whole conversation, chosen in Advanced, shows in Simple too.
  assert.equal(lines(importSummary(p, { ...recommended, turns: "up_to_last_user" }))[0], "Prompts: `messages`, whole conversation")
})

test("system prompts: a column, the conversation's own, or none", () => {
  const p: SummaryPreview = {
    ...base,
    columns: [col("system"), col("prompt"), col("chosen", "messages", true), col("rejected", "messages", true)],
    prompt_field: "prompt",
    system_field: "system",
    dropped: ["chosen", "rejected"],
  }
  assert.deepEqual(lines(importSummary(p, { ...recommended, source: "file" })), [
    "Prompts: `prompt` column",
    "System prompt: `system` column",
    "Left out: `chosen`, `rejected`",
  ])
  const conv: SummaryPreview = {
    ...base,
    columns: [col("conversations", "messages")],
    prompt_field: "conversations",
    has_conversations: true,
    prompts: [
      [
        { role: "system", content: "Be brief." },
        { role: "user", content: "Hi" },
      ],
    ],
  }
  assert.ok(lines(importSummary(conv, { ...recommended, source: "file" })).includes("System prompt: From the conversation"))
})

test("no prompt column: a warning (the Import button says how to fix it)", () => {
  const p: SummaryPreview = { ...base, columns: [col("id", "number"), col("response", "text", true)] }
  assert.deepEqual(lines(importSummary(p, { ...recommended, source: "file" })), [
    "Prompts: No prompt column found (warn)",
  ])
})

test("pasted text: how it was split; no split or system rows", () => {
  const p: SummaryPreview = { ...base, split_mode: "separator", prompts: [[{ role: "user", content: "a" }]] }
  assert.deepEqual(lines(importSummary(p, { ...recommended, source: "paste" })), ["Prompts: Split on --- lines"])
  assert.deepEqual(lines(importSummary({ ...p, split_mode: "whole" }, { ...recommended, source: "paste" })), [
    "Prompts: The whole text is one prompt",
  ])
})

test("settings changed in Advanced are listed, never hidden", () => {
  const p: SummaryPreview = { ...base, split_mode: "line" }
  const st: SummaryState = { ...recommended, source: "file", format: "markdown", offset: 10, limit: 500, dedupe: false, name: "Haiku prompts" }
  assert.deepEqual(lines(importSummary(p, st)), [
    "Prompts: One prompt per non-empty line",
    "Format: Markdown",
    "Skip prompts: 10",
    "Import at most: 500",
    "Duplicate prompts: Kept",
    "Name: Haiku prompts",
  ])
  const all = importSummary(
    { ...base, columns: [col("text")], prompt_field: "text", hf_splits: [{ config: "default", split: "train", num_rows: 1 }], hf_config: "default", hf_split: "train" },
    { ...recommended, allSplits: true, offset: 5 },
  )
  assert.deepEqual(lines(all), ["Prompts: `text` column", "Split: All splits", "System prompt: None", "Skip rows: 5"])
})

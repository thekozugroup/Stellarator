// Fills an empty Stellarator data folder with the sample data the store screenshots show.
//
// Through the app's own API it adds three endpoints, imports two prompt sets ("Quick checks",
// pasted, and openai/gsm8k from Hugging Face), runs three short runs on the mock model server,
// runs the long gsm8k run part way and pauses it, and makes a preference set with two pairs
// decided. Afterwards run polish-data.py on the data folder (store/README.md has the order).
//
//   node store/scripts/seed.mjs
//
// Needs: a server with an empty data folder at STELLARATOR_UI (default http://127.0.0.1:5180, the
// Vite dev server, or the server's own address), the mock model server at MOCK_URL (default
// http://127.0.0.1:8000/v1), and internet access for the Hugging Face import. Takes 2 to 4 minutes.

const BASE = (process.env.STELLARATOR_UI ?? "http://127.0.0.1:5180").replace(/\/$/, "")
const MOCK = process.env.MOCK_URL ?? "http://127.0.0.1:8000/v1"
// How far the long run goes before it is paused, and the seqs it is left at.
const LONG_DONE = Number(process.env.LONG_DONE ?? 4200)
const LONG_SEQS = 12

const QUICK_CHECKS = [
  "What is the capital of Australia, and why was it chosen over Sydney and Melbourne?",
  "Write a Python function that returns the n-th Fibonacci number in O(log n) time.",
  "Explain the difference between a mutex and a semaphore in two sentences.",
  "A train leaves at 9:40 and arrives at 13:05. How long is the trip?",
  "Summarize the plot of Hamlet in exactly three sentences.",
  "Prove that the square root of 2 is irrational.",
]

const sleep = (ms) => new Promise((r) => setTimeout(r, ms))

async function api(method, path, body) {
  const res = await fetch(BASE + path, {
    method,
    headers: body ? { "content-type": "application/json" } : undefined,
    body: body ? JSON.stringify(body) : undefined,
  })
  const text = await res.text()
  if (!res.ok) throw new Error(`${method} ${path}: ${res.status} ${text.slice(0, 300)}`)
  return text ? JSON.parse(text) : null
}

async function until(what, check, timeoutS = 600) {
  const end = Date.now() + timeoutS * 1000
  for (;;) {
    const v = await check()
    if (v) return v
    if (Date.now() > end) throw new Error(`timed out waiting for ${what}`)
    await sleep(1000)
  }
}

async function importSet(name, source, mapping) {
  const set = await api("POST", "/api/imports", { name, source, ...(mapping ? { mapping } : {}) })
  return until(`the import of ${name}`, async () => {
    const s = await api("GET", `/api/prompt-sets/${set.id}`)
    if (s.status === "failed") throw new Error(`import of ${name} failed: ${s.status_detail}`)
    return s.status === "ready" ? s : null
  })
}

async function run(body, untilDone = true) {
  const r = await api("POST", "/api/runs", body)
  console.log(`  run #${r.id}: ${r.name}`)
  if (untilDone) await until(`run ${r.id}`, async () => (await api("GET", `/api/runs/${r.id}`)).status === "completed")
  return r
}

async function main() {
  const existing = await api("GET", "/api/runs")
  if (existing.length) throw new Error("This data folder already has runs. Seed an empty one.")

  console.log("endpoints")
  const local = await api("POST", "/api/endpoints", { name: "LM Studio on this Mac", base_url: MOCK, default_concurrency: 8 })
  const spark = await api("POST", "/api/endpoints", { name: "DGX Spark vLLM", base_url: MOCK, default_concurrency: 32 })
  await api("POST", "/api/endpoints", { name: "Office llama.cpp box", base_url: "http://192.168.1.20:8080/v1", default_concurrency: 4 })
  for (const e of [local, spark]) await api("POST", `/api/endpoints/${e.id}/check`, {})

  console.log("prompt sets")
  const quick = await importSet("Quick checks", { kind: "paste", text: QUICK_CHECKS.join("\n---\n") })
  const gsmSource = { kind: "huggingface", dataset: "openai/gsm8k", config: "main", split: "train" }
  const preview = await api("POST", "/api/imports/preview", { source: gsmSource })
  const gsm = await importSet("gsm8k train", gsmSource, { prompt_field: preview.prompt_field })
  console.log(`  ${quick.name}: ${quick.prompt_count}; ${gsm.name}: ${gsm.prompt_count}`)

  console.log("runs")
  const base = { endpoint_id: local.id, prompt_set_id: quick.id, concurrency: 8 }
  const a = await run({ ...base, model: "mock-reasoning", params: { samples_per_prompt: 2, temperature: 0.7 } })
  const b = await run({ ...base, model: "mock-think-tags", params: { samples_per_prompt: 2, temperature: 0.7 } })
  await run({ ...base, model: "mock-truncating", concurrency: 4, params: { max_tokens: 64 } })

  // The long run: fast to begin with, then left at LONG_SEQS and paused part way.
  const long = await run(
    {
      name: "Distill gsm8k with long reasoning traces from the DGX Spark",
      endpoint_id: spark.id,
      prompt_set_id: gsm.id,
      model: "mock-reasoning-content",
      concurrency: 256,
      params: { temperature: 0.6, max_tokens: 8192 },
    },
    false,
  )
  await until(`${LONG_DONE} samples of run ${long.id}`, async () => {
    const r = await api("GET", `/api/runs/${long.id}`)
    process.stdout.write(`\r  ${r.done} of ${r.total}`)
    return r.done >= LONG_DONE
  })
  await api("PATCH", `/api/runs/${long.id}`, { concurrency: LONG_SEQS })
  await api("POST", `/api/runs/${long.id}/pause`, {})
  await until("the long run to pause", async () => (await api("GET", `/api/runs/${long.id}`)).status === "paused")
  console.log("\n  paused")

  console.log("preference set")
  const set = await api("POST", "/api/preference-sets", { name: "Reasoning vs think tags", run_ids: [a.id, b.id] })
  const items = (await api("GET", `/api/preference-sets/${set.id}/items?limit=50`)).items
  for (const item of items.filter((i) => i.prompt_idx < 2)) {
    const pick = (runId) => item.candidates.find((c) => c.run_id === runId && c.sample_idx === 0)
    await api("PUT", `/api/preference-sets/${set.id}/pairs/${item.prompt_id}`, {
      chosen_sample_id: pick(a.id).sample_id,
      rejected_sample_id: pick(b.id).sample_id,
    })
  }
  console.log("done. Next: stop the server, run polish-data.py on its data folder, start it again.")
}

main().catch((e) => {
  console.error(e)
  process.exit(1)
})

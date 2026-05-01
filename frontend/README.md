# Stellarator Frontend

Next.js 15 + React 19 + Tailwind v4 + shadcn/ui dashboard for the Stellarator
training-supervision backend.

## Pages

- `/` - dashboard. Stat cards + live runs table.
- `/runs/[id]` - run detail. Two-column layout. Live metrics over WebSocket.
- `/runs/compare` - overlay loss curves and diff hyperparameters across up to 6 runs. Shareable via `?ids=`.
- `/chat` - planning chat surface. OpenAI key in localStorage or Codex OAuth.
- `/research` - HF + arXiv search. "Cite to run" flow.
- `/settings` - viewer token and OpenAI key.

## Configuration

Copy `.env.example` to `.env.local`:

```
NEXT_PUBLIC_API_URL=http://localhost:8000
NEXT_PUBLIC_VIEWER_TOKEN=viewer-dev-token
```

The viewer token can also be set per-browser in Settings; the localStorage value
takes precedence over the env default.

## Develop

```
npm install
npm run dev
```

Or via the repo Compose stack: `docker compose up frontend`.

## Backend contract

The client expects these endpoints under `NEXT_PUBLIC_API_URL`:

- `GET  /v1/runs` -> `{ runs: Run[] }`
- `GET  /v1/runs/:id`
- `GET  /v1/runs/:id/metrics?since=`
- `GET  /v1/runs/:id/notes`
- `POST /v1/runs/:id/{cancel,pause,resume}`
- `WS   /v1/runs/:id/stream` -> `{ type: "metric" | "note" | "status", data }`
- `GET  /v1/stats/summary`
- `GET  /v1/research/papers/search?q=&source=`
- `POST /v1/research/runs/:id/cite`
- `POST /v1/oauth/codex/start` -> `{ url }`
- `POST /v1/chat/stream` -> SSE stream of `{type:"delta",text}` and `{type:"tool_call",...}`

Schemas live in `lib/types.ts` (zod-validated at the API boundary).

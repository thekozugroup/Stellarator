/** Thin fetch wrapper for the Stellarator API. Errors become ApiError with field messages. */

export class ApiError extends Error {
  status: number
  fields: Record<string, string>

  constructor(status: number, message: string, fields: Record<string, string> = {}) {
    super(message)
    this.status = status
    this.fields = fields
  }
}

function messageFor(status: number, detail: unknown): string {
  if (typeof detail === "string" && detail) return detail
  if (status === 404) return "Not found."
  if (status === 409) return "This conflicts with the current state. Refresh and try again."
  if (status >= 500) return "The Stellarator server hit an error. Check its logs, then retry."
  return `Request failed (${status}).`
}

async function request<T>(method: string, path: string, body?: unknown, init?: RequestInit): Promise<T> {
  let res: Response
  try {
    res = await fetch(path, {
      method,
      headers: body instanceof FormData || body === undefined ? undefined : { "Content-Type": "application/json" },
      body: body instanceof FormData ? body : body === undefined ? undefined : JSON.stringify(body),
      ...init,
    })
  } catch {
    throw new ApiError(0, "Can't reach the Stellarator server. Is it running?")
  }
  if (res.status === 204) return undefined as T
  const text = await res.text()
  let data: unknown = null
  try {
    data = text ? JSON.parse(text) : null
  } catch {
    data = text
  }
  if (!res.ok) {
    const obj = (data && typeof data === "object" ? data : {}) as { detail?: unknown; fields?: Record<string, string> }
    throw new ApiError(res.status, messageFor(res.status, obj.detail), obj.fields ?? {})
  }
  return data as T
}

export const api = {
  get: <T>(path: string, init?: RequestInit) => request<T>("GET", path, undefined, init),
  post: <T>(path: string, body?: unknown) => request<T>("POST", path, body ?? {}),
  put: <T>(path: string, body?: unknown) => request<T>("PUT", path, body ?? {}),
  patch: <T>(path: string, body?: unknown) => request<T>("PATCH", path, body ?? {}),
  del: <T = void>(path: string) => request<T>("DELETE", path),
  upload: <T>(path: string, file: File) => {
    const form = new FormData()
    form.append("file", file)
    return request<T>("POST", path, form)
  },
}

/** Build a query string, skipping null and undefined values. */
export function qs(params: Record<string, string | number | boolean | null | undefined>): string {
  const search = new URLSearchParams()
  for (const [k, v] of Object.entries(params)) {
    if (v !== null && v !== undefined && v !== "") search.set(k, String(v))
  }
  const s = search.toString()
  return s ? `?${s}` : ""
}

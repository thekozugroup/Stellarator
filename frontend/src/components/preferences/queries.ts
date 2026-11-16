import { useQuery, type QueryClient } from "@tanstack/react-query"
import { api, qs } from "@/lib/api"
import type { Page, PreferenceItem, PreferenceSet } from "@/lib/types"

export type ItemFilter = "undecided" | "decided" | "all"
export const ITEM_FILTERS: ItemFilter[] = ["undecided", "decided", "all"]

export const prefKeys = {
  list: ["preference-sets"] as const,
  set: (id: number) => ["preference-sets", id] as const,
  itemsOf: (id: number) => ["preference-items", id] as const,
  item: (id: number, filter: ItemFilter, offset: number) => ["preference-items", id, filter, offset] as const,
}

export function fetchItemAt(id: number, filter: ItemFilter, offset: number) {
  return api.get<Page<PreferenceItem>>(`/api/preference-sets/${id}/items${qs({ filter, offset, limit: 1 })}`)
}

export function usePreferenceSet(id: number) {
  return useQuery({
    queryKey: prefKeys.set(id),
    queryFn: () => api.get<PreferenceSet>(`/api/preference-sets/${id}`),
    enabled: Number.isFinite(id),
  })
}

/** Replace one prompt's item in every cached page of this set (after a save or a clear). */
export function patchCachedItem(qc: QueryClient, setId: number, item: PreferenceItem) {
  qc.setQueriesData<Page<PreferenceItem>>({ queryKey: prefKeys.itemsOf(setId) }, (page) => {
    if (!page) return page
    if (!page.items.some((it) => it.prompt_id === item.prompt_id)) return page
    return { ...page, items: page.items.map((it) => (it.prompt_id === item.prompt_id ? item : it)) }
  })
}

/** "A", "B", … "Z", "AA". The letter is how a candidate is referred to on screen and by key. */
export function letter(n: number): string {
  let s = ""
  let i = n
  do {
    s = String.fromCharCode(65 + (i % 26)) + s
    i = Math.floor(i / 26) - 1
  } while (i >= 0)
  return s
}

/** Drop the organisation prefix of a model id: "Qwen/Qwen3-32B" -> "Qwen3-32B". */
export function shortModel(model: string): string {
  const slash = model.lastIndexOf("/")
  return slash >= 0 ? model.slice(slash + 1) : model
}

const listFmt = new Intl.ListFormat(undefined, { style: "long", type: "conjunction" })
export function joinList(parts: string[]): string {
  return listFmt.format(parts)
}

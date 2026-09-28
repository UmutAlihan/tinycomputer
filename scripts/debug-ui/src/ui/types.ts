export type JournalEvent = {
  event: string
  seq?: number
  at?: string
  elapsed_ms?: number
  step?: string
  [key: string]: unknown
}

export type JournalRun = {
  id: string
  path: string
  events: JournalEvent[]
  errors: string[]
  source?: 'file' | 'server'
  loaded?: boolean
  modifiedAt?: string
  bytes?: number
}

export function formatDuration(ms: unknown): string {
  if (typeof ms !== 'number' || !Number.isFinite(ms)) return '—'
  if (ms < 1000) return `${Math.round(ms)} ms`
  return `${(ms / 1000).toFixed(ms >= 10_000 ? 1 : 2)} s`
}

export function pretty(value: unknown): string {
  return JSON.stringify(value, null, 2) ?? String(value)
}

export function asRecord(value: unknown): Record<string, unknown> | undefined {
  return value !== null && typeof value === 'object' && !Array.isArray(value)
    ? value as Record<string, unknown>
    : undefined
}

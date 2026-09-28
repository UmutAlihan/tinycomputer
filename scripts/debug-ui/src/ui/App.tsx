import { useEffect, useMemo, useRef, useState } from 'react'
import { Activity, AlertTriangle, ArrowDownToLine, ChevronDown, ChevronRight, Clock3, FileJson2, FolderOpen, RefreshCw, Search, Sparkles, X } from 'lucide-react'
import type { ChangeEvent, DragEvent } from 'react'
import { asRecord, formatDuration, pretty, type JournalEvent, type JournalRun } from './types'

const kinds = ['all', 'run', 'exchange', 'decision', 'survey', 'turn', 'observe', 'action', 'step', 'end']

function readSession(key: string): string | null {
  try { return sessionStorage.getItem(`jev-inspector:${key}`) } catch { return null }
}

function displayValue(value: unknown): string {
  if (typeof value === 'string') return value
  if (value === undefined || value === null) return ''
  return JSON.stringify(value)
}

function shapeCount(value: unknown): string {
  if (Array.isArray(value)) return String(value.length)
  if (typeof value === 'number') return String(value)
  const record = asRecord(value)
  return record ? String(Object.keys(record).length) : '—'
}

function tokenCount(value: unknown): number {
  return typeof value === 'number' && Number.isFinite(value) ? value : 0
}

function formatTokens(value: number): string {
  if (value >= 1_000_000) return `${(value / 1_000_000).toFixed(2).replace(/0+$/, '').replace(/\.$/, '')}M`
  if (value >= 10_000) return `${(value / 1_000).toFixed(1).replace(/\.0$/, '')}k`
  return value.toLocaleString()
}

function tokenDisplay(value: unknown): string {
  return typeof value === 'number' && Number.isFinite(value) ? formatTokens(value) : '—'
}

function parseJournal(text: string, path: string, id: string, source: 'file' | 'server'): JournalRun {
  const events: JournalEvent[] = []
  const errors: string[] = []
  text.split(/\r?\n/).forEach((line, index) => {
    if (!line.trim()) return
    try {
      const value: unknown = JSON.parse(line)
      if (asRecord(value) && typeof asRecord(value)?.event === 'string') events.push(value as JournalEvent)
      else errors.push(`Line ${index + 1}: expected a JSON object with an event field`)
    } catch (error) {
      errors.push(`Line ${index + 1}: ${error instanceof Error ? error.message : 'invalid JSON'}`)
    }
  })
  return { id, path, events, errors, source, loaded: true }
}

async function readFiles(files: FileList | File[]): Promise<JournalRun[]> {
  const journalFiles = Array.from(files).filter(file => file.name.endsWith('.jsonl') || file.name === 'journal.jsonl')
  const runs = await Promise.all(journalFiles.map(async file => {
    const path = file.webkitRelativePath || file.name
    return parseJournal(await file.text(), path, path.split('/').slice(-2, -1)[0] || file.name, 'file')
  }))
  return runs.sort((a, b) => (b.events[0]?.at ?? b.id).localeCompare(a.events[0]?.at ?? a.id))
}

async function readServerRun(id: string): Promise<JournalRun> {
  const response = await fetch(`/api/journal/runs/${encodeURIComponent(id)}`)
  if (!response.ok) throw new Error(`Could not load run ${id} (${response.status})`)
  const result = await response.json() as { id: string; text: string }
  return parseJournal(result.text, `${result.id}/journal.jsonl`, result.id, 'server')
}

function eventTitle(event: JournalEvent): string {
  switch (event.event) {
    case 'exchange': return `Jev exchange · ${String(event.model ?? 'model')}`
    case 'decision': return `Decision · step ${String(event.step ?? '—')}`
    case 'survey': return `Screen survey · step ${String(event.step ?? '—')}`
    case 'turn': return `Agent turn ${String(event.turn ?? '')} · step ${String(event.step ?? '—')}`
    case 'observe': return `Observation · ${String(event.part ?? 'screen')}`
    case 'action': return `${String(event.action ?? 'Action')} · ${displayValue(event.target)}`
    case 'step': return `${String(event.kind ?? 'Step')} · ${String(event.text ?? event.step ?? '')}`
    case 'run': return `${String(event.kind ?? 'Run')} started`
    case 'end': return `Run ended · ${String(event.stop ?? 'complete')}`
    default: return event.event
  }
}

function EventDetails({ event }: { event: JournalEvent }) {
  const [tab, setTab] = useState('summary')
  const request = asRecord(event.request)
  const tabs = ['summary', ...(request ? ['request'] : []), ...(event.answers !== undefined ? ['answers'] : []), ['raw']].flat()
  const summary = Object.fromEntries(Object.entries(event).filter(([key]) => !['request', 'answers'].includes(key)))
  const selectedValue = tab === 'request' ? event.request : tab === 'answers' ? event.answers : tab === 'raw' ? event : summary
  return <section className="detail-card">
    <div className="detail-head"><div><span className={`event-dot ${event.event}`} /><span className="eyebrow">{event.event} event</span><h2>{eventTitle(event)}</h2></div><span className="sequence">#{event.seq ?? '—'}</span></div>
    <div className="tabs">{tabs.map(item => <button className={tab === item ? 'active' : ''} key={item} onClick={() => setTab(item)}>{item}</button>)}</div>
    {tab === 'request' && request && <div className="request-meta">
      <Metric label="Questions" value={Array.isArray(event.questions) ? String(event.questions.length) : '—'} />
      <Metric label="Request size" value={typeof event.request_bytes === 'number' ? `${event.request_bytes.toLocaleString()} B` : '—'} />
      <Metric label="Latency" value={formatDuration(event.latency_ms)} />
      <Metric label="Attempts" value={String(event.attempts ?? '—')} />
      <Metric label="Input tokens" value={tokenDisplay(event.input_tokens)} />
      <Metric label="Output tokens" value={tokenDisplay(event.output_tokens)} />
    </div>}
    <JsonTree value={selectedValue} />
  </section>
}

function Metric({ label, value }: { label: string; value: string }) { return <div className="metric"><span>{label}</span><strong>{value}</strong></div> }

function JsonTree({ value }: { value: unknown }) {
  const [expanded, setExpanded] = useState(false)
  const [expandVersion, setExpandVersion] = useState(0)
  const setAll = (next: boolean) => { setExpanded(next); setExpandVersion(version => version + 1) }
  return <>
    <div className="json-tools"><span>Structured data</span><div><button onClick={() => setAll(true)}>Expand all</button><button onClick={() => setAll(false)}>Collapse all</button></div></div>
    <div className="json-tree"><JsonNode name={undefined} value={value} depth={0} expanded={expanded} expandVersion={expandVersion} /></div>
  </>
}

function JsonNode({ name, value, depth, expanded, expandVersion }: { name?: string; value: unknown; depth: number; expanded: boolean; expandVersion: number }) {
  const [open, setOpen] = useState(() => expandVersion === 0 ? depth < 2 : expanded)
  useEffect(() => { if (expandVersion > 0) setOpen(expanded) }, [expandVersion, expanded])
  const entries = Array.isArray(value) ? value.map((item, i) => [String(i), item] as const) : asRecord(value) ? Object.entries(value as Record<string, unknown>) : null
  const label = name === undefined ? '$' : name
  if (entries) return <div className="json-node" style={{ '--depth': depth } as React.CSSProperties}>
    <button className="json-row json-toggle" onClick={() => setOpen(!open)}>{open ? <ChevronDown size={14} /> : <ChevronRight size={14} />}<span className="json-key">{label}</span><span className="json-brace">{Array.isArray(value) ? `Array(${entries.length})` : `{${entries.length}}`}</span></button>
    {open && <div className="json-children">{entries.length ? entries.map(([key, child]) => <JsonNode key={key} name={key} value={child} depth={depth + 1} expanded={expanded} expandVersion={expandVersion} />) : <span className="empty-value">empty</span>}</div>}
  </div>
  const primitive = typeof value === 'string' ? <span className="json-string">&quot;{value}&quot;</span> : value === null ? <span className="json-null">null</span> : typeof value === 'number' ? <span className="json-number">{value}</span> : typeof value === 'boolean' ? <span className="json-bool">{String(value)}</span> : <span className="json-null">undefined</span>
  return <div className="json-row json-leaf" style={{ '--depth': depth } as React.CSSProperties}><span className="json-spacer" /><span className="json-key">{label}</span><span className="json-colon">:</span>{primitive}</div>
}

export default function App() {
  const [runs, setRuns] = useState<JournalRun[]>([])
  const [selectedRun, setSelectedRun] = useState<string | null>(() => readSession('run'))
  const [selectedSeq, setSelectedSeq] = useState<number | null>(() => {
    const stored = readSession('seq')
    return stored === null ? null : Number(stored)
  })
  const [query, setQuery] = useState(() => readSession('query') ?? '')
  const [eventFilter, setEventFilter] = useState(() => readSession('event-filter') ?? 'all')
  const [error, setError] = useState('')
  const [dragging, setDragging] = useState(false)
  const [serverLoading, setServerLoading] = useState(true)
  const [refreshing, setRefreshing] = useState(false)
  const [loadingPath, setLoadingPath] = useState<string | null>(null)
  const fileRef = useRef<HTMLInputElement>(null)
  const folderRef = useRef<HTMLInputElement>(null)
  const selectedRunRef = useRef<string | null>(selectedRun)
  const loadingPathRef = useRef<string | null>(null)
  const runsRef = useRef<JournalRun[]>(runs)
  runsRef.current = runs

  const refreshRuns = async (loadInitialSelection = false) => {
    if (loadInitialSelection) setServerLoading(true)
    else setRefreshing(true)
    setError('')
    let requestedPath: string | null = null
    try {
      const response = await fetch('/api/journal/runs')
      if (!response.ok) throw new Error(`Journal server returned ${response.status}`)
      const index = await response.json() as { id: string; modifiedAt: string; bytes: number }[]
      const previous = runsRef.current
      const available: JournalRun[] = index.map(run => {
        const path = `${run.id}/journal.jsonl`
        const existing = previous.find(item => item.path === path && item.loaded)
        if (existing) return { ...existing, modifiedAt: run.modifiedAt, bytes: run.bytes }
        return {
          id: run.id,
          path,
          events: [],
          errors: [],
          source: 'server',
          loaded: false,
          modifiedAt: run.modifiedAt,
          bytes: run.bytes,
        }
      })
      const merged = [...available, ...previous.filter(item => item.source !== 'server')]
      runsRef.current = merged
      setRuns(merged)

      let target = available.find(run => run.path === selectedRunRef.current)
      let selectedNewDefault = false
      if (!target && !selectedRunRef.current && loadInitialSelection) {
        target = available[0]
        selectedNewDefault = Boolean(target)
        if (target) {
          selectedRunRef.current = target.path
          setSelectedRun(target.path)
          setSelectedSeq(null)
        }
      }
      const oldSelection = target && previous.find(item => item.path === target?.path)
      const changedSelectedRun = !loadInitialSelection && target?.path === selectedRunRef.current && Boolean(oldSelection?.loaded) && (oldSelection?.modifiedAt !== target?.modifiedAt || oldSelection?.bytes !== target?.bytes)
      const shouldLoad = target && (loadInitialSelection ? !oldSelection?.loaded : changedSelectedRun)
      if (shouldLoad && target) {
        requestedPath = target.path
        loadingPathRef.current = target.path
        setLoadingPath(target.path)
        const loaded = await readServerRun(target.id)
        const nextRuns = runsRef.current.map(item => item.path === loaded.path ? loaded : item)
        runsRef.current = nextRuns
        setRuns(nextRuns)
        if (selectedNewDefault) setSelectedSeq(loaded.events.at(-1)?.seq ?? loaded.events.length - 1)
      }
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : 'Could not discover local journal files')
    } finally {
      if (requestedPath && loadingPathRef.current === requestedPath) {
        loadingPathRef.current = null
        setLoadingPath(null)
      }
      if (loadInitialSelection) {
        setServerLoading(false)
      } else setRefreshing(false)
    }
  }

  useEffect(() => { void refreshRuns(true) }, [])
  useEffect(() => {
    try {
      if (selectedRun) sessionStorage.setItem('jev-inspector:run', selectedRun)
      else sessionStorage.removeItem('jev-inspector:run')
      if (selectedSeq !== null) sessionStorage.setItem('jev-inspector:seq', String(selectedSeq))
      else sessionStorage.removeItem('jev-inspector:seq')
      sessionStorage.setItem('jev-inspector:query', query)
      sessionStorage.setItem('jev-inspector:event-filter', eventFilter)
    } catch { /* Session storage can be disabled in private browsing. */ }
  }, [selectedRun, selectedSeq, query, eventFilter])

  const current = runs.find(run => run.path === selectedRun) ?? runs[0]
  const selectedEvent = current?.events.find((event, index) => (event.seq ?? index) === selectedSeq) ?? current?.events.at(-1)
  const filteredEvents = useMemo(() => current?.events.filter((event, index) => {
    if (eventFilter !== 'all' && event.event !== eventFilter) return false
    if (!query.trim()) return true
    return pretty(event).toLowerCase().includes(query.toLowerCase())
  }) ?? [], [current, eventFilter, query])
  const stats = useMemo(() => {
    const events = current?.events ?? []
    const exchanges = events.filter(event => event.event === 'exchange')
    const failures = exchanges.filter(event => event.ok === false).length
    const inputTokens = exchanges.reduce((sum, event) => sum + tokenCount(event.input_tokens), 0)
    const outputTokens = exchanges.reduce((sum, event) => sum + tokenCount(event.output_tokens), 0)
    const wall = [...events].reverse().find(event => event.event === 'end')?.wall_ms
    return { exchanges: exchanges.length, failures, inputTokens, outputTokens, wall }
  }, [current])

  const chooseRun = async (run: JournalRun) => {
    selectedRunRef.current = run.path
    setSelectedRun(run.path)
    setQuery('')
    setSelectedSeq(run.events.at(-1)?.seq ?? run.events.length - 1)
    setError('')
    if (run.source !== 'server' || run.loaded) return
    loadingPathRef.current = run.path
    setLoadingPath(run.path)
    try {
      const loaded = await readServerRun(run.id)
      setRuns(previous => previous.map(item => item.path === run.path ? loaded : item))
      if (selectedRunRef.current === run.path) setSelectedSeq(loaded.events.at(-1)?.seq ?? loaded.events.length - 1)
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : 'Could not read that journal')
    } finally {
      if (loadingPathRef.current === run.path) {
        loadingPathRef.current = null
        setLoadingPath(null)
      }
    }
  }

  const load = async (files: FileList | File[]) => {
    setError('')
    try {
      const loaded = await readFiles(files)
      if (!loaded.length) { setError('No .jsonl files found. Select a journal file or the .jev-journal folder.'); return }
      const merged = [...loaded, ...runs.filter(run => !loaded.some(next => next.path === run.path))]
      runsRef.current = merged
      setRuns(merged)
      selectedRunRef.current = loaded[0].path
      setSelectedRun(loaded[0].path)
      setSelectedSeq(loaded[0].events.at(-1)?.seq ?? loaded[0].events.length - 1)
      setQuery('')
    } catch (reason) { setError(reason instanceof Error ? reason.message : 'Could not read those files') }
  }
  const onInput = (event: ChangeEvent<HTMLInputElement>) => { if (event.target.files) void load(event.target.files); event.target.value = '' }
  const onDrop = (event: DragEvent) => { event.preventDefault(); setDragging(false); void load(event.dataTransfer.files) }

  return <main className="app-shell" onDragOver={event => { event.preventDefault(); setDragging(true) }} onDragLeave={event => { if (event.currentTarget === event.target) setDragging(false) }} onDrop={onDrop}>
    <input ref={fileRef} hidden type="file" accept=".jsonl,application/json" multiple onChange={onInput} />
    <input ref={folderRef} hidden type="file" multiple onChange={onInput} {...({ webkitdirectory: '', directory: '' } as object)} />
    {dragging && <div className="drop-overlay"><div><ArrowDownToLine size={32} /><strong>Drop journal files to inspect</strong></div></div>}
    <header className="topbar"><div className="brand-mark"><Activity size={18} /></div><div className="brand-name">tinycomputer <span>/</span> <strong>jev inspector</strong></div><div className="topbar-right"><span className="local-badge"><i /> LOCAL DEV SERVER</span><a href="https://github.com/tinyhumansai/tinycomputer/blob/main/docs/jev-journal.md" target="_blank" rel="noreferrer">Journal format</a></div></header>
    <div className="workspace">
      <aside className="sidebar">
        <div className="sidebar-title"><span>JOURNAL RUNS</span><div className="sidebar-tools"><button className="refresh-button" onClick={() => void refreshRuns(false)} disabled={refreshing} title="Refresh the run list" aria-label="Refresh the run list"><RefreshCw size={13} /></button><span className="count-pill">{runs.length}</span></div></div>
        <button className="load-button" onClick={() => folderRef.current?.click()}><FolderOpen size={16} /> Open journal folder</button>
        <button className="load-file" onClick={() => fileRef.current?.click()}><FileJson2 size={15} /> Add JSONL files</button>
        {error && <div className="load-error"><AlertTriangle size={14} />{error}</div>}
        <div className="run-list">
          {serverLoading && !runs.length && <div className="empty-runs"><div className="folder-illustration"><FolderOpen size={23} /></div><strong>Checking local journal…</strong><p>Looking for <code>.jev-journal</code> in this checkout.</p></div>}
          {runs.map(run => {
            const start = run.events.find(item => item.event === 'run')
            const end = [...run.events].reverse().find(item => item.event === 'end')
            const active = current?.path === run.path
            return <button className={`run-item ${active ? 'selected' : ''}`} key={run.path} onClick={() => void chooseRun(run)}>
              <span className="run-type">{String(start?.kind ?? 'journal')}<span className={end ? 'status complete' : 'status'}>{end ? 'ended' : run.loaded ? 'open' : 'local'}</span></span>
              <strong>{String(start?.label ?? run.id)}</strong><small>{run.id}</small><span className="run-meta">{run.loaded ? `${run.events.length} events` : `${Math.round((run.bytes ?? 0) / 1024)} KB · ${run.modifiedAt ? new Date(run.modifiedAt).toLocaleString() : 'not loaded'}`}{run.errors.length ? ` · ${run.errors.length} parse errors` : ''}</span>
            </button>
          })}
          {!serverLoading && !runs.length && <div className="empty-runs"><div className="folder-illustration"><FolderOpen size={23} /></div><strong>No runs found</strong><p>Select <code>.jev-journal</code> above or drop JSONL files here.</p></div>}
        </div>
        <div className="privacy-note"><Sparkles size={14} /><span>The Vite dev server reads journals from this checkout. No cloud upload is used.</span></div>
      </aside>

      <section className="main-panel">
        {!current ? <div className="welcome"><div className="welcome-icon"><Activity size={25} /></div><span className="eyebrow">RUN EXPLORER</span><h1>See what happened<br />inside a run.</h1><p>Load a journal directory to inspect Jev exchanges, screen observations, actions, and timing in one place.</p><button className="primary-button" onClick={() => folderRef.current?.click()}><FolderOpen size={16} /> Open .jev-journal</button><div className="welcome-hint">No files leave this machine or its dev server</div></div> : <>
          <div className="run-header"><div className="breadcrumbs"><span>JOURNAL RUNS</span><ChevronRight size={13} /><span>{current.id}</span></div><div className="run-heading"><div><h1>{String(current.events.find(item => item.event === 'run')?.label ?? current.id)}</h1><p><span className="kind-label">{String(current.events.find(item => item.event === 'run')?.kind ?? 'journal')}</span><span>·</span><span>{String(current.events.find(item => item.event === 'run')?.model ?? 'unknown model')}</span><span>·</span><span>{current.path}</span></p></div><span className="end-status"><i /> {loadingPath === current.path ? 'LOADING RUN' : current.events.some(item => item.event === 'end') ? 'RUN COMPLETE' : current.loaded ? 'INCOMPLETE' : 'LOCAL JOURNAL'}</span></div>
            <div className="stat-strip"><div className="stat"><Clock3 size={15} /><span>Wall time</span><strong>{formatDuration(stats.wall)}</strong></div><div className="stat"><Sparkles size={15} /><span>Jev calls</span><strong>{stats.exchanges}</strong></div><div className="stat"><AlertTriangle size={15} /><span>Failed calls</span><strong className={stats.failures ? 'danger-text' : ''}>{stats.failures}</strong></div><div className="stat token-stat"><span>Input tokens</span><strong title={stats.inputTokens.toLocaleString()}>{formatTokens(stats.inputTokens)}</strong></div><div className="stat token-stat"><span>Output tokens</span><strong title={stats.outputTokens.toLocaleString()}>{formatTokens(stats.outputTokens)}</strong></div><div className="stat"><Activity size={15} /><span>Events</span><strong>{current.events.length}</strong></div></div>
          </div>
          <div className="explorer-grid">
            <section className="timeline-panel">
              <div className="timeline-title"><div><h2>Event timeline</h2><span>{filteredEvents.length} of {current.events.length} events</span></div><div className="event-filter"><select value={eventFilter} onChange={event => setEventFilter(event.target.value)}>{kinds.map(kind => <option key={kind} value={kind}>{kind === 'all' ? 'All events' : kind}</option>)}</select></div></div>
              <label className="search-box"><Search size={15} /><input placeholder="Search events, prompts, answers…" value={query} onChange={event => setQuery(event.target.value)} />{query && <button aria-label="Clear search" onClick={() => setQuery('')}><X size={14} /></button>}</label>
              {current.errors.length > 0 && <div className="parse-errors"><AlertTriangle size={14} /><span>{current.errors.length} malformed line(s): {current.errors.slice(0, 2).join(' · ')}</span></div>}
              <div className="timeline-scroll"><div className="timeline-line" />{loadingPath === current.path && !current.loaded ? <div className="no-results">Loading journal events…</div> : filteredEvents.map((event, index) => {
                const seq = event.seq ?? current.events.indexOf(event)
                const chosen = seq === selectedSeq
                return <button key={`${seq}-${index}`} className={`timeline-event ${chosen ? 'chosen' : ''}`} onClick={() => setSelectedSeq(seq)}><span className={`event-dot ${event.event}`} /><span className="timeline-content"><strong>{eventTitle(event)}</strong><small>{event.step ? `Step ${event.step} · ` : ''}{event.at ? new Date(event.at).toLocaleTimeString() : `#${seq}`}</small><span className="timeline-summary">{event.event === 'exchange' ? `${formatDuration(event.latency_ms)} · ${event.request_bytes ?? '—'} B · ${tokenDisplay(event.input_tokens)} in / ${tokenDisplay(event.output_tokens)} out${event.ok === false ? ' · failed' : ''}` : event.event === 'step' ? `${String(event.outcome ?? '—')} · ${formatDuration(event.wall_ms)}` : event.event === 'action' ? `${event.ok ? 'success' : 'failed'} · ${formatDuration(event.wall_ms)}` : event.event === 'observe' ? `${formatDuration(event.wall_ms)} · ${event.candidates ?? '—'} candidates` : event.event === 'survey' ? `${shapeCount(event.regions)} regions · ${shapeCount(event.distractions)} distractions` : event.event === 'turn' ? `${event.decisions ?? '—'} decisions · ${formatDuration(event.wall_ms)}` : `+${formatDuration(event.elapsed_ms)}`}</span></span><span className="timeline-index">{String(seq).padStart(2, '0')}</span></button>
              })}{!filteredEvents.length && <div className="no-results">No events match this filter.</div>}</div>
            </section>
            <div className="details-scroll">{selectedEvent ? <EventDetails key={`${current.path}:${selectedSeq}`} event={selectedEvent} /> : <div className="empty-detail">{loadingPath === current.path ? 'Loading journal events…' : 'Choose an event to inspect it.'}</div>}{current.errors.length > 0 && <div className="parse-card"><AlertTriangle size={14} />Some lines could not be parsed. Valid events are still available above.</div>}</div>
          </div>
        </>}
      </section>
    </div>
  </main>
}

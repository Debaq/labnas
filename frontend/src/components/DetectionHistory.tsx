import { useCallback, useEffect, useMemo, useState } from 'react'
import { ChevronDown, ChevronLeft, ChevronRight, ChevronUp, Download, Loader2, Send, Video, X } from 'lucide-react'
import { cameraRecordingUrl, fetchDetections, type DetectionRow, type DetectorRule } from '../api'
import { useEvent } from '../events/useEvents'

const card = { backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }
const input = { backgroundColor: 'var(--bg-tertiary)', color: 'var(--text-primary)', border: '1px solid var(--border)' }
const PAGE = 200

/** Detecciones seguidas de la misma regla, juntas en un solo evento */
interface DetectionEvent {
  key: string
  rule: string
  items: DetectionRow[] // la mas nueva primero
}

function localDay(iso: string): string {
  const d = new Date(iso)
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`
}

function dayTitle(day: string): string {
  const today = localDay(new Date().toISOString())
  const y = new Date()
  y.setDate(y.getDate() - 1)
  if (day === today) return 'Hoy'
  if (day === localDay(y.toISOString())) return 'Ayer'
  return new Date(`${day}T12:00`).toLocaleDateString('es-CL', { weekday: 'long', day: 'numeric', month: 'long' })
}

const hhmm = (iso: string) => new Date(iso).toLocaleTimeString('es-CL', { hour: '2-digit', minute: '2-digit' })
const hhmmss = (iso: string) => new Date(iso).toLocaleTimeString('es-CL')

/**
 * Agrupa por regla: una deteccion se suma al evento abierto de su regla si ocurrio a menos
 * de `gap` de la anterior (persona que sigue frente a la camara).
 */
function groupEvents(rows: DetectionRow[], gapMs: (rule: string) => number): DetectionEvent[] {
  const events: DetectionEvent[] = []
  const open = new Map<string, DetectionEvent>()
  for (const r of rows) {
    const ev = open.get(r.detector_id)
    const oldest = ev?.items[ev.items.length - 1]
    if (ev && oldest && new Date(oldest.at).getTime() - new Date(r.at).getTime() <= gapMs(r.detector_id)) {
      ev.items.push(r)
    } else {
      const n: DetectionEvent = { key: r.id, rule: r.detector_id, items: [r] }
      events.push(n)
      open.set(r.detector_id, n)
    }
  }
  return events
}

/** Estado del aviso por Telegram de una o varias detecciones */
function TelegramBadge({ items }: { items: DetectionRow[] }) {
  const asked = items.filter((d) => d.tg_sent !== null)
  if (asked.length === 0) return null
  const failed = asked.filter((d) => !d.tg_sent)
  const ok = failed.length === 0
  const title = ok
    ? `Aviso enviado por Telegram (${asked.length === 1 ? `${asked[0].tg_sent} chat(s)` : `${asked.length} avisos`})`
    : `${failed.length} de ${asked.length} avisos no llegaron: ${failed[0].tg_error ?? 'error desconocido'}`
  return (
    <span title={title} className="inline-flex items-center gap-1 text-[11px] px-1.5 py-0.5 rounded-full whitespace-nowrap"
      style={{ color: ok ? 'var(--success)' : 'var(--danger)', border: '1px solid currentColor' }}>
      <Send size={10} /> {ok ? 'Enviado' : failed.length === asked.length ? 'No enviado' : `${failed.length} sin enviar`}
    </span>
  )
}

interface Props {
  rules: DetectorRule[]
  cameras: { id: string; name: string }[]
  ruleLabel: (r: DetectorRule) => string
}

/** Historial de detecciones: por dia, agrupado en eventos, con filtros y vista previa */
export default function DetectionHistory({ rules, cameras, ruleLabel }: Props) {
  const [rows, setRows] = useState<DetectionRow[]>([])
  const [loading, setLoading] = useState(true)
  const [more, setMore] = useState(false)
  const [camera, setCamera] = useState('')
  const [rule, setRule] = useState('')
  const [day, setDay] = useState('')
  const [open, setOpen] = useState<Set<string>>(new Set())
  const [viewer, setViewer] = useState<{ list: DetectionRow[]; i: number } | null>(null)

  const filter = useMemo(() => ({ camera, rule, day }), [camera, rule, day])

  const load = useCallback(async () => {
    try {
      const r = await fetchDetections({ ...filter, limit: PAGE })
      setRows(r)
      setMore(r.length === PAGE)
    } catch {
      setRows([])
    } finally {
      setLoading(false)
    }
  }, [filter])

  useEffect(() => { setLoading(true); load() }, [load])
  useEvent('cameras.detection', load, load)

  const loadMore = async () => {
    const last = rows[rows.length - 1]
    if (!last) return
    const r = await fetchDetections({ ...filter, before: last.at, limit: PAGE }).catch(() => [])
    setRows((prev) => [...prev, ...r])
    setMore(r.length === PAGE)
  }

  const ruleById = useMemo(() => new Map(rules.map((r) => [r.id, r])), [rules])
  // Mismo evento si la siguiente llega antes de 1,5 pausas de la regla (minimo 5 min)
  const gap = useCallback((id: string) => Math.max(5 * 60, (ruleById.get(id)?.cooldown_s ?? 0) * 1.5 + 60) * 1000, [ruleById])

  const days = useMemo(() => {
    const out: { day: string; events: DetectionEvent[] }[] = []
    for (const ev of groupEvents(rows, gap)) {
      const d = localDay(ev.items[0].at)
      const last = out[out.length - 1]
      if (last?.day === d) last.events.push(ev)
      else out.push({ day: d, events: [ev] })
    }
    return out
  }, [rows, gap])

  const toggle = (k: string) => setOpen((s) => {
    const n = new Set(s)
    if (n.has(k)) n.delete(k)
    else n.add(k)
    return n
  })

  const describe = (id: string, fallback: string) => {
    const r = ruleById.get(id)
    return r ? ruleLabel(r) : `${fallback} (regla eliminada)`
  }

  return (
    <div className="space-y-3">
      <div className="flex gap-2 flex-wrap text-xs">
        <select value={camera} onChange={(e) => setCamera(e.target.value)} className="px-2 py-1.5 rounded-lg outline-none" style={input}>
          <option value="">Todas las cámaras</option>
          {cameras.map((c) => <option key={c.id} value={c.id}>{c.name}</option>)}
        </select>
        <select value={rule} onChange={(e) => setRule(e.target.value)} className="px-2 py-1.5 rounded-lg outline-none max-w-[16rem]" style={input}>
          <option value="">Todas las reglas</option>
          {rules.map((r) => <option key={r.id} value={r.id}>{r.camera} · {ruleLabel(r)}</option>)}
        </select>
        <input type="date" value={day} onChange={(e) => setDay(e.target.value)} className="px-2 py-1.5 rounded-lg outline-none" style={input} />
        {(camera || rule || day) && (
          <button onClick={() => { setCamera(''); setRule(''); setDay('') }} className="px-2" style={{ color: 'var(--text-secondary)' }}>Quitar filtros</button>
        )}
      </div>

      {loading ? (
        <div className="flex justify-center py-8"><Loader2 size={20} className="animate-spin" style={{ color: 'var(--accent)' }} /></div>
      ) : rows.length === 0 ? (
        <p className="text-sm p-4 rounded-xl" style={{ ...card, color: 'var(--text-secondary)' }}>
          {camera || rule || day ? 'Nada con estos filtros.' : 'Todavía no hay detecciones.'}
        </p>
      ) : (
        <>
          {days.map(({ day: d, events }) => (
            <div key={d}>
              <h3 className="text-xs font-semibold uppercase tracking-wide mb-1.5 capitalize" style={{ color: 'var(--text-secondary)' }}>
                {dayTitle(d)} · {events.reduce((n, e) => n + e.items.length, 0)} detecciones
              </h3>
              <div className="rounded-xl divide-y" style={{ ...card, borderColor: 'var(--card-border)' }}>
                {events.map((ev) => {
                  const first = ev.items[0]
                  const last = ev.items[ev.items.length - 1]
                  const many = ev.items.length > 1
                  const expanded = open.has(ev.key)
                  const withPhoto = ev.items.filter((x) => x.photo_id || x.clip_id)
                  const cover = ev.items.find((x) => x.photo_id)
                  return (
                    <div key={ev.key} className="p-2" style={{ borderColor: 'var(--border)' }}>
                      <div className="flex items-center gap-3">
                        <button onClick={() => withPhoto.length && setViewer({ list: withPhoto, i: 0 })}
                          className="w-20 h-12 shrink-0 rounded-md overflow-hidden flex items-center justify-center" style={{ backgroundColor: 'var(--bg-tertiary)' }}
                          title={withPhoto.length ? 'Ver' : 'Sin foto'}>
                          {cover?.photo_id
                            ? <img src={cameraRecordingUrl(cover.photo_id, 'thumb')} alt="" loading="lazy" className="w-full h-full object-cover" />
                            : first.clip_id ? <Video size={16} style={{ color: 'var(--text-secondary)' }} /> : null}
                        </button>
                        <div className="flex-1 min-w-0 text-xs">
                          <div className="flex items-center gap-2 flex-wrap">
                            <span className="font-mono" style={{ color: 'var(--text-primary)' }}>
                              {many ? `${hhmm(last.at)}–${hhmm(first.at)}` : hhmmss(first.at)}
                            </span>
                            <span className="font-medium" style={{ color: 'var(--text-primary)' }}>{first.camera_name}</span>
                            <span style={{ color: 'var(--text-secondary)' }}>{describe(ev.rule, first.kind)}</span>
                            {many && (
                              <span className="px-1.5 py-0.5 rounded-full text-[11px]" style={{ backgroundColor: 'var(--accent-alpha)', color: 'var(--accent)' }}>
                                {ev.items.length} veces
                              </span>
                            )}
                            <TelegramBadge items={ev.items} />
                          </div>
                          <div className="truncate" style={{ color: 'var(--text-secondary)' }}>{first.labels}</div>
                        </div>
                        {many && (
                          <button onClick={() => toggle(ev.key)} className="p-1 shrink-0" style={{ color: 'var(--text-secondary)' }} title={expanded ? 'Ocultar' : 'Ver cada una'}>
                            {expanded ? <ChevronUp size={16} /> : <ChevronDown size={16} />}
                          </button>
                        )}
                      </div>
                      {expanded && (
                        <div className="grid grid-cols-3 sm:grid-cols-5 lg:grid-cols-8 gap-2 mt-2">
                          {ev.items.map((x) => (
                            <button key={x.id} onClick={() => { const i = withPhoto.findIndex((w) => w.id === x.id); if (i >= 0) setViewer({ list: withPhoto, i }) }}
                              className="text-left rounded-md overflow-hidden" style={{ backgroundColor: 'var(--bg-tertiary)' }}>
                              <div className="aspect-video flex items-center justify-center">
                                {x.photo_id
                                  ? <img src={cameraRecordingUrl(x.photo_id, 'thumb')} alt="" loading="lazy" className="w-full h-full object-cover" />
                                  : x.clip_id ? <Video size={14} style={{ color: 'var(--text-secondary)' }} /> : null}
                              </div>
                              <div className="px-1 py-0.5 text-[10px] flex items-center justify-between gap-1" style={{ color: 'var(--text-secondary)' }}>
                                {hhmmss(x.at)}
                                {x.tg_sent !== null && <span style={{ color: x.tg_sent ? 'var(--success)' : 'var(--danger)' }} title={x.tg_error ?? 'Enviado por Telegram'}>●</span>}
                              </div>
                            </button>
                          ))}
                        </div>
                      )}
                    </div>
                  )
                })}
              </div>
            </div>
          ))}
          {more && (
            <button onClick={loadMore} className="w-full py-2 rounded-lg text-xs" style={{ ...card, color: 'var(--accent)' }}>Cargar más</button>
          )}
        </>
      )}

      {viewer && <DetectionViewer list={viewer.list} start={viewer.i} describe={describe} onClose={() => setViewer(null)} />}
    </div>
  )
}

/** Visor: foto o clip en grande, con flechas para pasar a la siguiente */
function DetectionViewer({ list, start, describe, onClose }: {
  list: DetectionRow[]
  start: number
  describe: (rule: string, kind: string) => string
  onClose: () => void
}) {
  const [i, setI] = useState(start)
  // Deteccion en la que se eligio ver el clip en vez de la foto
  const [clipOf, setClipOf] = useState<string | null>(null)
  const d = list[i]
  const showClip = !!d && (clipOf === d.id || !d.photo_id)

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose()
      if (e.key === 'ArrowLeft') setI((x) => Math.min(list.length - 1, x + 1))
      if (e.key === 'ArrowRight') setI((x) => Math.max(0, x - 1))
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [list.length, onClose])

  if (!d) return null
  const media = showClip && d.clip_id ? d.clip_id : d.photo_id
  return (
    <div className="fixed inset-0 z-50 bg-black/80 flex items-center justify-center p-4" onClick={onClose}>
      <div className="w-full max-w-5xl space-y-2" onClick={(e) => e.stopPropagation()}>
        <div className="flex items-center gap-3 text-sm text-white flex-wrap">
          <span className="font-medium">{d.camera_name}</span>
          <span className="opacity-80">{new Date(d.at).toLocaleString('es-CL')}</span>
          <span className="opacity-80">{describe(d.detector_id, d.kind)} · {d.labels}</span>
          <TelegramBadge items={[d]} />
          {d.tg_error && <span className="text-xs" style={{ color: 'var(--danger)' }}>{d.tg_error}</span>}
          <span className="ml-auto flex items-center gap-2">
            {d.photo_id && d.clip_id && (
              <button onClick={() => setClipOf(showClip ? null : d.id)} className="px-2 py-1 rounded text-xs border border-white/40">
                {showClip ? 'Ver foto' : 'Ver clip'}
              </button>
            )}
            {media && <a href={cameraRecordingUrl(media)} title="Descargar" className="p-1"><Download size={16} /></a>}
            <button onClick={onClose} className="p-1" title="Cerrar"><X size={18} /></button>
          </span>
        </div>
        <div className="relative flex items-center justify-center bg-black rounded-lg min-h-[40vh]">
          {media && (showClip && d.clip_id
            ? <video key={d.clip_id} src={cameraRecordingUrl(d.clip_id, 'inline')} controls autoPlay className="max-h-[75vh] w-full" />
            : <img key={media} src={cameraRecordingUrl(media, 'inline')} alt={d.labels} className="max-h-[75vh] w-full object-contain" />)}
          {i < list.length - 1 && (
            <button onClick={() => setI(i + 1)} className="absolute left-2 p-2 rounded-full bg-black/60 text-white" title="Anterior"><ChevronLeft size={20} /></button>
          )}
          {i > 0 && (
            <button onClick={() => setI(i - 1)} className="absolute right-2 p-2 rounded-full bg-black/60 text-white" title="Siguiente"><ChevronRight size={20} /></button>
          )}
        </div>
        {list.length > 1 && <div className="text-center text-xs text-white/70">{list.length - i} de {list.length}</div>}
      </div>
    </div>
  )
}

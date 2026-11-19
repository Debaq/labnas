import { useCallback, useEffect, useState } from 'react'
import { Link2, Search, Loader2, ShieldCheck, ShieldAlert, Download, Plus, Ban, FileCheck2, Anchor } from 'lucide-react'
import {
  fetchLedger, verifyLedger, addLedgerNote, annulLedgerEntry, checkLedgerFiles, anchorLedger, downloadLedger,
  type LedgerEntry, type LedgerVerify, type LedgerFileCheck,
} from '../api'
import { useToast } from './ToastContext'

const PAGE = 100

const FILE_STATUS: Record<LedgerFileCheck['status'], { label: string; color: string }> = {
  igual: { label: 'Sin cambios', color: 'var(--success)' },
  cambiado: { label: 'Cambió', color: 'var(--danger)' },
  no_existe: { label: 'Ya no existe', color: 'var(--warning)' },
  sin_acceso: { label: 'Sin acceso', color: 'var(--text-secondary)' },
}

function formatTime(ts: string): string {
  const d = new Date(ts)
  return isNaN(d.getTime()) ? ts : d.toLocaleString()
}

interface NoteData { title?: string; text?: string; reason?: string; files?: { path: string; sha256: string; size: number }[] }

function parseData(e: LedgerEntry): NoteData {
  try { return JSON.parse(e.data) as NoteData } catch { return {} }
}

/** Admin: bitacora encadenada con huellas. Nada se edita ni se borra; un error se anula con un registro nuevo. */
export default function LedgerSection() {
  const { addToast } = useToast()
  const [entries, setEntries] = useState<LedgerEntry[]>([])
  const [status, setStatus] = useState<LedgerVerify | null>(null)
  const [q, setQ] = useState('')
  const [loading, setLoading] = useState(false)
  const [hasMore, setHasMore] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [newNote, setNewNote] = useState<{ title: string; subject: string; text: string; files: string } | null>(null)
  const [saving, setSaving] = useState(false)
  const [annulling, setAnnulling] = useState<{ seq: number; reason: string } | null>(null)
  const [checks, setChecks] = useState<Record<number, LedgerFileCheck[]>>({})
  const [anchorText, setAnchorText] = useState<string | null>(null)

  const refreshStatus = useCallback(() => {
    verifyLedger().then(setStatus).catch((e) => setError(e instanceof Error ? e.message : String(e)))
  }, [])

  const load = useCallback(async (append: boolean, beforeSeq?: number) => {
    setLoading(true)
    setError(null)
    try {
      const page = await fetchLedger({ q: q.trim(), before_seq: beforeSeq, limit: PAGE })
      setEntries((prev) => (append ? [...prev, ...page] : page))
      setHasMore(page.length === PAGE)
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    } finally {
      setLoading(false)
    }
  }, [q])

  useEffect(() => {
    load(false)
    refreshStatus()
    // Solo al montar; el filtro se aplica con el boton / Enter
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  const afterWrite = () => { load(false); refreshStatus() }

  const saveNote = async () => {
    if (!newNote) return
    setSaving(true)
    try {
      const files = newNote.files.split('\n').map((f) => f.trim()).filter(Boolean)
      const e = await addLedgerNote({ title: newNote.title, subject: newNote.subject, text: newNote.text, files })
      addToast(`Registro #${e.seq} agregado`, 'success')
      setNewNote(null)
      afterWrite()
    } catch (e) {
      addToast(e instanceof Error ? e.message : String(e), 'error')
    } finally {
      setSaving(false)
    }
  }

  const saveAnnul = async () => {
    if (!annulling) return
    setSaving(true)
    try {
      const e = await annulLedgerEntry(annulling.seq, annulling.reason)
      addToast(`Registro #${annulling.seq} anulado por #${e.seq}`, 'success')
      setAnnulling(null)
      afterWrite()
    } catch (e) {
      addToast(e instanceof Error ? e.message : String(e), 'error')
    } finally {
      setSaving(false)
    }
  }

  const runCheck = async (seq: number) => {
    try {
      const res = await checkLedgerFiles(seq)
      setChecks((prev) => ({ ...prev, [seq]: res }))
    } catch (e) {
      addToast(e instanceof Error ? e.message : String(e), 'error')
    }
  }

  const runAnchor = async () => {
    try {
      const r = await anchorLedger()
      setAnchorText(r.text)
      refreshStatus()
      addToast(r.telegram_sent > 0
        ? `Huella enviada por Telegram (${r.telegram_sent} chat${r.telegram_sent > 1 ? 's' : ''})`
        : 'Telegram no está configurado: guarda el texto fuera del NAS', r.telegram_sent > 0 ? 'success' : 'info')
    } catch (e) {
      addToast(e instanceof Error ? e.message : String(e), 'error')
    }
  }

  const inputStyle = { backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }
  const btnStyle = { backgroundColor: 'var(--bg-tertiary)', color: 'var(--text-primary)', border: '1px solid var(--border)' }
  const short = (h: string) => `${h.slice(0, 10)}…${h.slice(-6)}`

  return (
    <section>
      <div className="flex items-center gap-3 mb-4">
        <Link2 size={22} style={{ color: 'var(--accent)' }} />
        <h2 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>
          Bitácora inalterable
        </h2>
      </div>
      <div
        className="rounded-xl p-6 space-y-4"
        style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}
      >
        <p className="text-sm" style={{ color: 'var(--text-secondary)' }}>
          Cada registro queda encadenado al anterior con su huella digital. Nada se edita ni se borra:
          un error se corrige con un registro nuevo que lo anula. Las notas pueden guardar la huella de
          archivos del NAS (facturas, actas, fotos) para demostrar después que no cambiaron.
        </p>

        {status && (
          <div
            className="rounded-lg p-3 text-sm space-y-1"
            style={{ border: `1px solid ${status.ok ? 'var(--success)' : 'var(--danger)'}` }}
          >
            <div className="flex items-center gap-2 font-medium" style={{ color: status.ok ? 'var(--success)' : 'var(--danger)' }}>
              {status.ok ? <ShieldCheck size={16} /> : <ShieldAlert size={16} />}
              {status.ok
                ? `Cadena íntegra: ${status.count} registros`
                : `Cadena alterada desde el registro #${status.broken_at}: ${status.reason}`}
            </div>
            <div className="font-mono text-xs break-all" style={{ color: 'var(--text-secondary)' }}>
              Huella final: {status.head_hash}
            </div>
            <div className="text-xs" style={{ color: 'var(--text-secondary)' }}>
              {status.last_anchor
                ? `Último ancla: registro #${status.last_anchor.seq}, ${formatTime(status.last_anchor.at)}${status.last_anchor.via === 'telegram' ? ' (por Telegram)' : ' (solo texto mostrado, sin Telegram)'}`
                : 'Nunca anclada. Anclar envía la huella final fuera del servidor; así se nota si alguien reescribe la cadena.'}
            </div>
          </div>
        )}

        {anchorText && (
          <div className="rounded-lg p-3 text-xs space-y-2" style={{ backgroundColor: 'var(--bg-tertiary)' }}>
            <p style={{ color: 'var(--text-secondary)' }}>Guarda este texto fuera del NAS (correo a Finanzas, acta):</p>
            <pre className="whitespace-pre-wrap break-all font-mono" style={{ color: 'var(--text-primary)' }}>{anchorText}</pre>
            <button
              onClick={() => navigator.clipboard.writeText(anchorText).then(() => addToast('Copiado', 'success')).catch(() => {})}
              className="px-2 py-1 rounded" style={btnStyle}
            >
              Copiar
            </button>
          </div>
        )}

        <div className="flex flex-wrap gap-2">
          <input
            value={q}
            onChange={(e) => setQ(e.target.value)}
            onKeyDown={(e) => { if (e.key === 'Enter') load(false) }}
            placeholder="Buscar en registros"
            className="flex-1 min-w-48 px-3 py-2 rounded-lg text-sm outline-none"
            style={inputStyle}
          />
          <button
            onClick={() => load(false)}
            disabled={loading}
            className="flex items-center gap-1 px-3 py-2 rounded-lg text-sm disabled:opacity-50"
            style={{ backgroundColor: 'var(--accent)', color: 'var(--bg-primary)' }}
          >
            {loading ? <Loader2 size={15} className="animate-spin" /> : <Search size={15} />}
            Filtrar
          </button>
          <button onClick={() => setNewNote({ title: '', subject: '', text: '', files: '' })} className="flex items-center gap-1 px-3 py-2 rounded-lg text-sm" style={btnStyle}>
            <Plus size={15} /> Nota
          </button>
          <button onClick={runAnchor} disabled={!status?.ok || !status.count} className="flex items-center gap-1 px-3 py-2 rounded-lg text-sm disabled:opacity-50" style={btnStyle}>
            <Anchor size={15} /> Anclar
          </button>
          <button
            onClick={() => downloadLedger().catch((e) => addToast(e instanceof Error ? e.message : String(e), 'error'))}
            className="flex items-center gap-1 px-3 py-2 rounded-lg text-sm" style={btnStyle}
          >
            <Download size={15} /> Exportar
          </button>
        </div>

        {newNote && (
          <div className="rounded-lg p-4 space-y-2" style={{ border: '1px solid var(--border)' }}>
            <input value={newNote.title} onChange={(e) => setNewNote({ ...newNote, title: e.target.value })}
              placeholder="Título (ej.: Aprobación compra impresora 3D)" maxLength={200}
              className="w-full px-3 py-2 rounded-lg text-sm outline-none" style={inputStyle} />
            <input value={newNote.subject} onChange={(e) => setNewNote({ ...newNote, subject: e.target.value })}
              placeholder="Asunto o proyecto (opcional)" maxLength={200}
              className="w-full px-3 py-2 rounded-lg text-sm outline-none" style={inputStyle} />
            <textarea value={newNote.text} onChange={(e) => setNewNote({ ...newNote, text: e.target.value })}
              placeholder="Detalle: qué se decidió, quién, montos, fuente de financiamiento…" rows={4}
              className="w-full px-3 py-2 rounded-lg text-sm outline-none" style={inputStyle} />
            <textarea value={newNote.files} onChange={(e) => setNewNote({ ...newNote, files: e.target.value })}
              placeholder="Archivos del NAS, una ruta por línea (se guarda su huella)" rows={2}
              className="w-full px-3 py-2 rounded-lg text-sm outline-none font-mono" style={inputStyle} />
            <p className="text-xs" style={{ color: 'var(--warning)' }}>
              Una vez guardada, la nota no se puede editar ni borrar; solo anular con un registro nuevo.
            </p>
            <div className="flex gap-2 justify-end">
              <button onClick={() => setNewNote(null)} className="px-3 py-1.5 rounded-lg text-sm" style={btnStyle}>Cancelar</button>
              <button onClick={saveNote} disabled={saving || !newNote.title.trim()}
                className="px-3 py-1.5 rounded-lg text-sm disabled:opacity-50"
                style={{ backgroundColor: 'var(--accent)', color: 'var(--bg-primary)' }}>
                Registrar
              </button>
            </div>
          </div>
        )}

        {error && <p className="text-sm" style={{ color: 'var(--danger)' }}>{error}</p>}

        <div className="space-y-2 overflow-auto max-h-[560px]">
          {entries.map((e) => {
            const d = parseData(e)
            const annulled = e.annulled_by !== null
            return (
              <div key={e.seq} className="rounded-lg p-3 text-sm" style={{ border: '1px solid var(--border)', opacity: annulled ? 0.6 : 1 }}>
                <div className="flex flex-wrap items-center gap-2">
                  <span className="font-mono text-xs px-1.5 rounded" style={{ backgroundColor: 'var(--bg-tertiary)', color: 'var(--text-secondary)' }}>#{e.seq}</span>
                  <span className="text-xs uppercase" style={{ color: e.kind === 'anulacion' ? 'var(--danger)' : 'var(--accent)' }}>{e.kind}</span>
                  <span className="font-medium" style={{ color: 'var(--text-primary)', textDecoration: annulled ? 'line-through' : undefined }}>
                    {e.kind === 'anulacion' ? `Anula el registro #${e.annuls}` : (d.title ?? '')}
                  </span>
                  {e.subject && <span className="text-xs" style={{ color: 'var(--text-secondary)' }}>· {e.subject}</span>}
                  <span className="ml-auto text-xs whitespace-nowrap" style={{ color: 'var(--text-secondary)' }}>
                    {e.username} · {formatTime(e.timestamp)}
                  </span>
                </div>
                {(d.text || d.reason) && (
                  <p className="mt-1 whitespace-pre-wrap" style={{ color: 'var(--text-primary)' }}>{d.text || d.reason}</p>
                )}
                {annulled && <p className="mt-1 text-xs" style={{ color: 'var(--danger)' }}>Anulado por el registro #{e.annulled_by}</p>}
                {d.files && d.files.length > 0 && (
                  <ul className="mt-2 space-y-0.5 text-xs font-mono">
                    {d.files.map((f, i) => {
                      const c = checks[e.seq]?.[i]
                      return (
                        <li key={i} className="break-all" style={{ color: 'var(--text-secondary)' }}>
                          {f.path} · {short(f.sha256)}
                          {c && <span className="ml-2 font-sans" style={{ color: FILE_STATUS[c.status].color }}>{FILE_STATUS[c.status].label}</span>}
                        </li>
                      )
                    })}
                  </ul>
                )}
                <div className="mt-2 flex flex-wrap items-center gap-2 text-xs">
                  <span className="font-mono" style={{ color: 'var(--text-secondary)' }} title={e.hash}>huella {short(e.hash)}</span>
                  {d.files && d.files.length > 0 && (
                    <button onClick={() => runCheck(e.seq)} className="flex items-center gap-1 px-2 py-0.5 rounded" style={btnStyle}>
                      <FileCheck2 size={13} /> Comprobar archivos
                    </button>
                  )}
                  {!annulled && e.kind !== 'anulacion' && (
                    <button onClick={() => setAnnulling({ seq: e.seq, reason: '' })} className="flex items-center gap-1 px-2 py-0.5 rounded" style={{ ...btnStyle, color: 'var(--danger)' }}>
                      <Ban size={13} /> Anular
                    </button>
                  )}
                </div>
                {annulling?.seq === e.seq && (
                  <div className="mt-2 flex flex-wrap gap-2">
                    <input value={annulling.reason} onChange={(ev) => setAnnulling({ ...annulling, reason: ev.target.value })}
                      placeholder="Motivo de la anulación" autoFocus
                      className="flex-1 min-w-48 px-3 py-1.5 rounded-lg text-sm outline-none" style={inputStyle} />
                    <button onClick={() => setAnnulling(null)} className="px-3 py-1.5 rounded-lg text-sm" style={btnStyle}>Cancelar</button>
                    <button onClick={saveAnnul} disabled={saving || !annulling.reason.trim()}
                      className="px-3 py-1.5 rounded-lg text-sm disabled:opacity-50"
                      style={{ backgroundColor: 'var(--danger)', color: 'var(--bg-primary)' }}>
                      Anular
                    </button>
                  </div>
                )}
              </div>
            )
          })}
          {entries.length === 0 && !loading && (
            <p className="py-6 text-center text-sm" style={{ color: 'var(--text-secondary)' }}>Sin registros</p>
          )}
        </div>

        {hasMore && (
          <button
            onClick={() => load(true, entries[entries.length - 1]?.seq)}
            disabled={loading}
            className="w-full py-2 rounded-lg text-sm disabled:opacity-50"
            style={{ backgroundColor: 'var(--bg-tertiary)', color: 'var(--text-secondary)' }}
          >
            Cargar más
          </button>
        )}
      </div>
    </section>
  )
}

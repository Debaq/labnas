import { useEffect, useState } from 'react'
import { Copy, Loader2, Trash2, X } from 'lucide-react'
import {
  addGrant, deleteGrant, fetchAccess, fetchUsernames, publicUrl, setVisibility,
  type AccessInfo, type RecordKind, type Visibility,
} from '../../api'
import { errorMessage } from '../../lib/errors'
import { VISIBILITY } from './levels'

const input = { backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }

function day(ts: number) {
  return new Date(ts * 1000).toLocaleDateString('es-CL')
}

function plusDays(n: number) {
  const d = new Date(Date.now() + n * 86400000)
  return d.toISOString().slice(0, 10)
}

/** Ventana para decidir quien ve un registro: privado, con autorizacion o publico */
function AccessModal({ kind, id, onClose, onChanged }: { kind: RecordKind; id: string; onClose: () => void; onChanged?: (v: Visibility) => void }) {
  const [info, setInfo] = useState<AccessInfo | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const [users, setUsers] = useState<string[]>([])
  const [who, setWho] = useState('')
  const [until, setUntil] = useState(plusDays(30))
  const [note, setNote] = useState('')
  const [copied, setCopied] = useState(false)

  useEffect(() => {
    fetchAccess(kind, id).then(setInfo).catch(e => setError(errorMessage(e)))
    fetchUsernames().then(setUsers).catch(() => {})
  }, [kind, id])

  async function run(fn: () => Promise<AccessInfo | void>) {
    setBusy(true)
    setError(null)
    try {
      const r = await fn()
      if (r) { setInfo(r); onChanged?.(r.visibility) }
    } catch (e) {
      setError(errorMessage(e))
    } finally {
      setBusy(false)
    }
  }

  function grant() {
    if (!who || !until) return
    // Hasta el final del dia elegido
    const ts = Math.floor(new Date(`${until}T23:59:59`).getTime() / 1000)
    run(async () => {
      const r = await addGrant(kind, id, who, ts, note)
      setWho('')
      setNote('')
      return r
    })
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50" onClick={onClose}>
      <div className="rounded-xl p-6 w-full max-w-lg mx-4 space-y-4 max-h-[90vh] overflow-y-auto" style={{ backgroundColor: 'var(--bg-secondary)', border: '1px solid var(--border)' }} onClick={e => e.stopPropagation()}>
        <div className="flex items-start justify-between gap-3">
          <div>
            <h3 className="text-base font-semibold" style={{ color: 'var(--text-primary)' }}>Quien puede verlo</h3>
            {info && <p className="text-xs mt-0.5" style={{ color: 'var(--text-secondary)' }}>{info.title}</p>}
          </div>
          <button onClick={onClose} aria-label="Cerrar"><X size={18} style={{ color: 'var(--text-secondary)' }} /></button>
        </div>

        {error && <p className="text-sm" style={{ color: 'var(--danger)' }}>{error}</p>}
        {!info && !error && <Loader2 size={20} className="animate-spin" style={{ color: 'var(--accent)' }} />}

        {info && (
          <>
            <div className="space-y-2">
              {(Object.keys(VISIBILITY) as Visibility[]).map(v => {
                const meta = VISIBILITY[v]
                const Icon = meta.icon
                const active = info.visibility === v
                return (
                  <button
                    key={v}
                    disabled={busy}
                    onClick={() => !active && run(() => setVisibility(kind, id, v))}
                    className="w-full flex items-start gap-3 p-3 rounded-lg text-left"
                    style={{ border: `1px solid ${active ? 'var(--accent)' : 'var(--border)'}`, backgroundColor: active ? 'var(--bg-tertiary)' : 'transparent' }}
                  >
                    <Icon size={18} style={{ color: meta.color, marginTop: 2 }} />
                    <span>
                      <span className="block text-sm" style={{ color: 'var(--text-primary)', fontWeight: active ? 600 : 400 }}>{meta.label}</span>
                      <span className="block text-xs" style={{ color: 'var(--text-secondary)' }}>{meta.hint}</span>
                    </span>
                  </button>
                )
              })}
            </div>

            {info.visibility === 'publico' && info.public_token && (
              <div className="space-y-1">
                <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>Enlace publico (deja de funcionar si vuelves a privado):</p>
                <div className="flex gap-2">
                  <input readOnly value={publicUrl(info.public_token)} className="flex-1 rounded-lg px-2 py-1.5 text-xs font-mono" style={input} onFocus={e => e.target.select()} />
                  <button
                    onClick={() => { navigator.clipboard?.writeText(publicUrl(info.public_token!)); setCopied(true); setTimeout(() => setCopied(false), 1500) }}
                    className="px-3 rounded-lg text-xs flex items-center gap-1" style={{ backgroundColor: 'var(--bg-tertiary)', color: 'var(--text-primary)' }}
                  >
                    <Copy size={13} /> {copied ? 'Copiado' : 'Copiar'}
                  </button>
                </div>
              </div>
            )}

            {info.visibility !== 'privado' && (
              <div className="space-y-2 pt-2" style={{ borderTop: '1px solid var(--border)' }}>
                <p className="text-sm font-medium" style={{ color: 'var(--text-primary)' }}>Personas autorizadas</p>
                {info.grants.length === 0 && <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>Nadie todavia.</p>}
                {info.grants.map(g => (
                  <div key={g.id} className="flex items-center justify-between gap-2 text-sm">
                    <span style={{ color: g.active ? 'var(--text-primary)' : 'var(--text-secondary)' }}>
                      {g.username}
                      <span className="text-xs ml-2" style={{ color: g.active ? 'var(--text-secondary)' : 'var(--danger)' }}>
                        {g.active ? `hasta ${day(g.expires_at)}` : `vencio ${day(g.expires_at)}`}
                      </span>
                      {g.note && <span className="block text-xs" style={{ color: 'var(--text-secondary)' }}>{g.note}</span>}
                    </span>
                    <button disabled={busy} onClick={() => run(async () => { await deleteGrant(g.id); return fetchAccess(kind, id) })} aria-label={`Quitar a ${g.username}`}>
                      <Trash2 size={14} style={{ color: 'var(--danger)' }} />
                    </button>
                  </div>
                ))}
              </div>
            )}

            <div className="space-y-2 pt-2" style={{ borderTop: '1px solid var(--border)' }}>
              <p className="text-sm font-medium" style={{ color: 'var(--text-primary)' }}>Autorizar a alguien</p>
              <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>
                Vera una ficha de solo lectura en «Compartido conmigo», aunque su rol no tenga este modulo. Para alguien de fuera, crea antes una cuenta con rol Usuario externo o Auditor.
              </p>
              <div className="grid grid-cols-2 gap-2">
                <select value={who} onChange={e => setWho(e.target.value)} className="rounded-lg px-2 py-1.5 text-sm" style={input}>
                  <option value="">Persona…</option>
                  {users.map(u => <option key={u} value={u}>{u}</option>)}
                </select>
                <input type="date" value={until} min={plusDays(0)} onChange={e => setUntil(e.target.value)} className="rounded-lg px-2 py-1.5 text-sm" style={input} aria-label="Hasta" />
              </div>
              <input value={note} onChange={e => setNote(e.target.value)} maxLength={300} placeholder="Motivo (opcional): rendicion ANID, auditoria…" className="w-full rounded-lg px-2 py-1.5 text-sm" style={input} />
              <div className="flex justify-end">
                <button disabled={busy || !who || !until} onClick={grant} className="px-4 py-2 rounded-lg text-sm disabled:opacity-50" style={{ backgroundColor: 'var(--accent)', color: '#fff' }}>
                  Autorizar
                </button>
              </div>
            </div>
            <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>Cada cambio queda en la auditoria y cada vista en el registro de accesos.</p>
          </>
        )}
      </div>
    </div>
  )
}

/**
 * Insignia con el nivel de acceso de un registro; al hacer clic abre la ventana para
 * cambiarlo. Mostrarla solo a quien puede gestionarlo (el servidor lo vuelve a revisar).
 */
export default function AccessButton({ kind, id, level = 'privado', compact = false, onChanged }: {
  kind: RecordKind
  id: string
  level?: Visibility
  /** Solo el icono */
  compact?: boolean
  onChanged?: (v: Visibility) => void
}) {
  const [open, setOpen] = useState(false)
  const meta = VISIBILITY[level]
  const Icon = meta.icon
  return (
    <>
      <button
        onClick={e => { e.stopPropagation(); setOpen(true) }}
        title={`${meta.label}: ${meta.hint}. Clic para cambiar`}
        className="inline-flex items-center gap-1 px-2 py-0.5 rounded-md text-xs"
        style={{ color: meta.color, border: '1px solid var(--border)' }}
      >
        <Icon size={13} />{!compact && meta.label}
      </button>
      {open && <AccessModal kind={kind} id={id} onClose={() => setOpen(false)} onChanged={onChanged} />}
    </>
  )
}

import { useCallback, useEffect, useState } from 'react'
import { CheckCircle2, Loader2 } from 'lucide-react'
import {
  fetchManual, fetchManualSignatures, fetchManualVersion, publishManual, signManual,
  type ManualSignStatus, type ManualVersion, type ManualView,
} from '../../../api'
import { useEvent } from '../../../events/useEvents'
import { useToast } from '../../ToastContext'
import { errorMessage } from '../../../lib/errors'
import type { FinanceTabProps } from '../../../lib/finance'
import { cardStyle, fmtDate, inputStyle } from '../../../lib/financeRendicion'

/** Manual de procedimientos: version vigente, firma de cada integrante, historial y publicacion */
export default function ManualSection({ staff }: FinanceTabProps) {
  const { addToast } = useToast()
  const [view, setView] = useState<ManualView | null>(null)
  const [sigs, setSigs] = useState<ManualSignStatus | null>(null)
  const [shown, setShown] = useState<ManualVersion | null>(null)
  const [loading, setLoading] = useState(true)
  const [busy, setBusy] = useState(false)
  const [editing, setEditing] = useState(false)
  const [title, setTitle] = useState('')
  const [body, setBody] = useState('')
  const [summary, setSummary] = useState('')

  const load = useCallback(async () => {
    try {
      const v = await fetchManual()
      setView(v)
      setShown(v.current)
      setSigs(await fetchManualSignatures())
    } catch (e) {
      addToast(errorMessage(e), 'error')
    } finally {
      setLoading(false)
    }
  }, [addToast])
  // eslint-disable-next-line react-hooks/set-state-in-effect
  useEffect(() => { void load() }, [load])
  useEvent('finance', () => { void load() }, () => { void load() })

  const current = view?.current ?? null
  const sign = async () => {
    if (!current) return
    setBusy(true)
    try {
      await signManual(current.id)
      addToast('Firmado: quedo constancia en la bitacora', 'success')
      await load()
    } catch (e) { addToast(errorMessage(e), 'error') } finally { setBusy(false) }
  }
  const publish = async () => {
    setBusy(true)
    try {
      const v = await publishManual({ title, body, summary })
      addToast(`Version ${v.version} publicada; se aviso a quienes deben firmar`, 'success')
      setEditing(false)
      setSummary('')
      await load()
    } catch (e) { addToast(errorMessage(e), 'error') } finally { setBusy(false) }
  }
  const open = async (id: string) => {
    try { setShown(await fetchManualVersion(id)) } catch (e) { addToast(errorMessage(e), 'error') }
  }

  if (loading) return <div className="flex justify-center py-8"><Loader2 className="animate-spin" /></div>
  return (
    <div className="space-y-4">
      <div className="rounded-xl p-4 space-y-3" style={cardStyle}>
        {!current ? (
          <p className="text-sm" style={{ color: 'var(--text-secondary)' }}>Todavia no hay un manual publicado.</p>
        ) : (
          <>
            <div className="flex items-start justify-between gap-3 flex-wrap">
              <div>
                <h2 className="font-semibold" style={{ color: 'var(--text-primary)' }}>{shown?.title ?? current.title}</h2>
                <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>
                  Version {shown?.version ?? current.version}{shown && shown.id !== current.id ? ' (anterior)' : ' (vigente)'} · publicada por {shown?.published_by} el {fmtDate(shown?.published_at)} · huella {shown?.sha256.slice(0, 16)}…
                </p>
                {shown?.summary && <p className="text-xs mt-1" style={{ color: 'var(--text-secondary)' }}>Cambios: {shown.summary}</p>}
              </div>
              {view?.my_signed_at ? (
                <span className="inline-flex items-center gap-1 text-sm" style={{ color: 'var(--success, #16a34a)' }}><CheckCircle2 size={16} /> Firmado el {fmtDate(view.my_signed_at)}</span>
              ) : (
                <button onClick={sign} disabled={busy} className="px-4 py-2 rounded-lg text-sm" style={{ backgroundColor: 'var(--accent)', color: 'var(--accent-text, #fff)' }}>
                  Confirmo que lei la version {current.version}
                </button>
              )}
            </div>
            <pre className="whitespace-pre-wrap text-sm rounded-lg p-3 max-h-[28rem] overflow-y-auto" style={{ ...inputStyle, fontFamily: 'inherit' }}>{shown?.body}</pre>
          </>
        )}
      </div>

      {sigs?.version && (
        <div className="rounded-xl p-4 text-sm space-y-2" style={{ ...cardStyle, color: 'var(--text-primary)' }}>
          <h3 className="font-semibold">Firmas de la version {sigs.version.version}: {sigs.signed.length} de {sigs.required.length}</h3>
          {sigs.pending.length > 0 && <p><b>Falta firmar:</b> {sigs.pending.join(', ')}</p>}
          <ul className="text-xs" style={{ color: 'var(--text-secondary)' }}>
            {sigs.signed.map((s) => <li key={s.username}>{s.username} · {fmtDate(s.signed_at)}</li>)}
          </ul>
        </div>
      )}

      {view && view.versions.length > 0 && (
        <div className="rounded-xl p-4 text-sm" style={{ ...cardStyle, color: 'var(--text-primary)' }}>
          <h3 className="font-semibold mb-2">Historial de versiones</h3>
          <ul className="space-y-1">
            {view.versions.map((v) => (
              <li key={v.id}>
                <button onClick={() => void open(v.id)} className="text-left" style={{ color: 'var(--accent)' }}>Version {v.version}</button>
                <span className="text-xs" style={{ color: 'var(--text-secondary)' }}> · {fmtDate(v.published_at)} · {v.published_by} · {v.signatures} firmas · {v.summary}</span>
              </li>
            ))}
          </ul>
        </div>
      )}

      {staff && (
        <div className="rounded-xl p-4 space-y-2 text-sm" style={cardStyle}>
          {!editing ? (
            <button onClick={() => { setEditing(true); setTitle(current?.title ?? ''); setBody(current?.body ?? '') }} className="px-3 py-1.5 rounded-lg" style={inputStyle}>
              Publicar una version nueva
            </button>
          ) : (
            <>
              <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>Una version publicada no se edita ni se borra: la nueva queda en el historial y todos deben firmarla de nuevo.</p>
              <input value={title} onChange={(e) => setTitle(e.target.value)} placeholder="Titulo" maxLength={200} className="w-full rounded-lg px-3 py-1.5" style={inputStyle} />
              <textarea value={body} onChange={(e) => setBody(e.target.value)} rows={14} placeholder="Texto del manual (Markdown)" className="w-full rounded-lg px-3 py-2 font-mono text-xs" style={inputStyle} />
              <input value={summary} onChange={(e) => setSummary(e.target.value)} placeholder="Resumen de los cambios" maxLength={1000} className="w-full rounded-lg px-3 py-1.5" style={inputStyle} />
              <div className="flex gap-2">
                <button onClick={publish} disabled={busy} className="px-4 py-1.5 rounded-lg" style={{ backgroundColor: 'var(--accent)', color: 'var(--accent-text, #fff)' }}>Publicar</button>
                <button onClick={() => setEditing(false)} className="px-3 py-1.5 rounded-lg" style={inputStyle}>Cancelar</button>
              </div>
            </>
          )}
        </div>
      )}
    </div>
  )
}

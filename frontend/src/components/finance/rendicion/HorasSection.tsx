import { useCallback, useEffect, useState } from 'react'
import { Loader2 } from 'lucide-react'
import {
  createHours, decideHours, fetchHours, fetchHoursSummary, finFileUrl, fileToDataUrl,
  type HourRow, type HourSummaryRow,
} from '../../../api'
import { useAuth } from '../../../auth/AuthContext'
import { useEvent } from '../../../events/useEvents'
import { useToast } from '../../ToastContext'
import { errorMessage } from '../../../lib/errors'
import { money, type FinanceTabProps } from '../../../lib/finance'
import { HOUR_STATUS_LABEL, cardStyle, inputStyle, thisMonth } from '../../../lib/financeRendicion'

/** Horas por persona y proyecto, con su contrato o boleta; aprobarlas las cuenta como gastado */
export default function HorasSection({ projectId, projects, staff, canUse }: FinanceTabProps) {
  const { addToast } = useToast()
  const { user } = useAuth()
  const [rows, setRows] = useState<HourRow[]>([])
  const [summary, setSummary] = useState<HourSummaryRow[]>([])
  const [loading, setLoading] = useState(true)
  const [busy, setBusy] = useState(false)
  const [project, setProject] = useState('')
  const [month, setMonth] = useState(thisMonth())
  const [hours, setHours] = useState('')
  const [amount, setAmount] = useState('')
  const [notes, setNotes] = useState('')
  const [file, setFile] = useState<File | null>(null)

  const load = useCallback(async () => {
    try {
      const [r, s] = await Promise.all([fetchHours(projectId), fetchHoursSummary(projectId)])
      setRows(r)
      setSummary(s)
    } catch (e) { addToast(errorMessage(e), 'error') } finally { setLoading(false) }
  }, [projectId, addToast])
  // eslint-disable-next-line react-hooks/set-state-in-effect
  useEffect(() => { void load() }, [load])
  useEvent('finance', () => { void load() }, () => { void load() })

  const create = async () => {
    setBusy(true)
    try {
      await createHours({
        project_id: projectId ?? project, month, hours: Number(hours), amount: Number(amount) || 0, notes,
        file: file ? { name: file.name, data: await fileToDataUrl(file) } : null,
      })
      addToast('Horas registradas', 'success')
      setHours(''); setAmount(''); setNotes(''); setFile(null)
      await load()
    } catch (e) { addToast(errorMessage(e), 'error') } finally { setBusy(false) }
  }
  const decide = async (id: string, action: 'approve' | 'reject' | 'annul') => {
    let note = ''
    if (action !== 'approve') {
      note = window.prompt(action === 'reject' ? 'Motivo del rechazo' : 'Motivo de la anulacion') ?? ''
      if (!note.trim()) return
    }
    try {
      await decideHours(id, action, note)
      await load()
    } catch (e) { addToast(errorMessage(e), 'error') }
  }

  if (loading) return <div className="flex justify-center py-8"><Loader2 className="animate-spin" /></div>
  const target = projectId ?? project
  return (
    <div className="space-y-4 text-sm" style={{ color: 'var(--text-primary)' }}>
      {canUse && (
        <div className="rounded-xl p-4 space-y-2" style={cardStyle}>
          <h2 className="font-semibold">Registrar mis horas del mes</h2>
          <div className="flex flex-wrap gap-2 items-end">
            {!projectId && (
              <select value={project} onChange={(e) => setProject(e.target.value)} className="rounded-lg px-2 py-1.5" style={inputStyle}>
                <option value="">Proyecto…</option>
                {projects.map((p) => <option key={p.id} value={p.id}>{p.title}</option>)}
              </select>
            )}
            <input type="month" value={month} onChange={(e) => setMonth(e.target.value)} className="rounded-lg px-2 py-1.5" style={inputStyle} />
            <input type="number" min="0" step="0.5" value={hours} onChange={(e) => setHours(e.target.value)} placeholder="Horas" className="rounded-lg px-2 py-1.5 w-24" style={inputStyle} />
            <input type="number" min="0" value={amount} onChange={(e) => setAmount(e.target.value)} placeholder="Monto $" className="rounded-lg px-2 py-1.5 w-32" style={inputStyle} />
            <input value={notes} onChange={(e) => setNotes(e.target.value)} placeholder="Detalle" maxLength={1000} className="rounded-lg px-2 py-1.5 flex-1 min-w-[10rem]" style={inputStyle} />
            <input type="file" accept=".pdf,.jpg,.jpeg,.png,.xml" onChange={(e) => setFile(e.target.files?.[0] ?? null)} className="text-xs" />
            <button onClick={create} disabled={busy || !target || !hours} className="px-4 py-1.5 rounded-lg" style={{ backgroundColor: 'var(--accent)', color: 'var(--accent-text, #fff)' }}>Registrar</button>
          </div>
          <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>Con monto, adjunte el contrato o la boleta de honorarios. Lo aprueba quien gestiona Finanzas o el responsable del proyecto, nunca usted mismo.</p>
        </div>
      )}

      <div className="rounded-xl p-3 overflow-x-auto" style={cardStyle}>
        <h2 className="font-semibold mb-2">Resumen por proyecto y persona</h2>
        <table className="w-full text-xs">
          <thead><tr className="text-left" style={{ color: 'var(--text-secondary)' }}><th className="py-1">Proyecto</th><th>Persona</th><th className="text-right">Horas aprobadas</th><th className="text-right">Monto aprobado</th><th className="text-right">Horas pendientes</th></tr></thead>
          <tbody>
            {summary.map((s) => (
              <tr key={s.project_id + s.username} style={{ borderTop: '1px solid var(--border)' }}>
                <td className="py-1">{s.project}</td><td>{s.username}</td><td className="text-right">{s.approved_hours}</td><td className="text-right">{money(s.approved_amount)}</td><td className="text-right">{s.pending_hours}</td>
              </tr>
            ))}
            {summary.length === 0 && <tr><td colSpan={5} className="py-2" style={{ color: 'var(--text-secondary)' }}>Sin horas registradas.</td></tr>}
          </tbody>
        </table>
      </div>

      <div className="rounded-xl p-3 overflow-x-auto" style={cardStyle}>
        <h2 className="font-semibold mb-2">Registros</h2>
        <table className="w-full text-xs">
          <thead><tr className="text-left" style={{ color: 'var(--text-secondary)' }}><th className="py-1">Mes</th><th>Persona</th><th>Proyecto</th><th className="text-right">Horas</th><th className="text-right">Monto</th><th>Respaldo</th><th>Estado</th><th /></tr></thead>
          <tbody>
            {rows.map((h) => {
              const mine = h.username === user?.username || h.created_by === user?.username
              return (
                <tr key={h.id} style={{ borderTop: '1px solid var(--border)' }}>
                  <td className="py-1">{h.month}</td><td>{h.username}</td><td>{h.project}</td><td className="text-right">{h.hours}</td><td className="text-right">{money(h.amount)}</td>
                  <td>{h.file ? <a href={finFileUrl(h.file.id)} target="_blank" rel="noreferrer" style={{ color: 'var(--accent)' }}>{h.file.name}</a> : '—'}</td>
                  <td>{HOUR_STATUS_LABEL[h.status]}{h.decided_by ? ` por ${h.decided_by}` : ''}{h.decision_note ? ` (${h.decision_note})` : ''}</td>
                  <td className="whitespace-nowrap text-right">
                    {h.status === 'pendiente' && !mine && (
                      <>
                        <button onClick={() => void decide(h.id, 'approve')} className="px-2 py-0.5 rounded mr-1" style={{ color: 'var(--accent)' }}>Aprobar</button>
                        <button onClick={() => void decide(h.id, 'reject')} className="px-2 py-0.5 rounded" style={{ color: 'var(--danger)' }}>Rechazar</button>
                      </>
                    )}
                    {staff && (h.status === 'pendiente' || h.status === 'aprobada') && (
                      <button onClick={() => void decide(h.id, 'annul')} className="px-2 py-0.5 rounded" style={{ color: 'var(--text-secondary)' }}>Anular</button>
                    )}
                  </td>
                </tr>
              )
            })}
          </tbody>
        </table>
      </div>
    </div>
  )
}

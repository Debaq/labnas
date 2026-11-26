import { useCallback, useEffect, useState } from 'react'
import { Loader2, Plus } from 'lucide-react'
import { annulIncome, createIncome, fetchIncomes, finFileUrl, fileToDataUrl, type Income } from '../../../api'
import { useEvent } from '../../../events/useEvents'
import { useToast } from '../../ToastContext'
import { errorMessage } from '../../../lib/errors'
import { money, type FinanceTabProps } from '../../../lib/finance'
import { cardStyle, fmtDay, inputStyle, todayYmd } from '../../../lib/financeBudget'

const TH = 'text-left py-2 px-2 font-medium text-xs'
const field = 'block w-full mt-1 px-2 py-1.5 rounded-lg text-sm outline-none'
const lab = { color: 'var(--text-secondary)' }

function IncomeForm({ projectId, onDone, onCancel }: { projectId: string; onDone: () => void; onCancel: () => void }) {
  const { addToast } = useToast()
  const [amount, setAmount] = useState('')
  const [origin, setOrigin] = useState('')
  const [destination, setDestination] = useState('')
  const [date, setDate] = useState(todayYmd())
  const [notes, setNotes] = useState('')
  const [file, setFile] = useState<File | null>(null)
  const ready = Number(amount) > 0 && origin.trim()
  const submit = async () => {
    try {
      await createIncome({
        project_id: projectId, amount: Number(amount), origin, destination, received_on: date, notes,
        ...(file ? { file: await fileToDataUrl(file), file_name: file.name } : {}),
      })
      addToast('Ingreso registrado', 'success')
      onDone()
    } catch (e) {
      addToast(errorMessage(e), 'error')
    }
  }
  return (
    <div className="rounded-xl p-4 space-y-3" style={cardStyle}>
      <h3 className="text-sm font-semibold" style={{ color: 'var(--text-primary)' }}>Nuevo ingreso por servicios</h3>
      <div className="grid gap-3 sm:grid-cols-4">
        <label className="text-xs" style={lab}>Monto<input type="number" min={1} value={amount} onChange={(e) => setAmount(e.target.value)} className={field} style={inputStyle} /></label>
        <label className="text-xs" style={lab}>Origen (quién paga y por qué)<input value={origin} onChange={(e) => setOrigin(e.target.value)} maxLength={120} className={field} style={inputStyle} /></label>
        <label className="text-xs" style={lab}>Destino del dinero<input value={destination} onChange={(e) => setDestination(e.target.value)} maxLength={120} className={field} style={inputStyle} /></label>
        <label className="text-xs" style={lab}>Fecha<input type="date" max={todayYmd()} value={date} onChange={(e) => setDate(e.target.value)} className={field} style={inputStyle} /></label>
      </div>
      <div className="grid gap-3 sm:grid-cols-2">
        <label className="text-xs" style={lab}>Notas (opcional)<input value={notes} onChange={(e) => setNotes(e.target.value)} maxLength={300} className={field} style={inputStyle} /></label>
        <label className="text-xs" style={lab}>Respaldo (opcional)<input type="file" accept="application/pdf,image/jpeg,image/png" onChange={(e) => setFile(e.target.files?.[0] ?? null)} className={field} style={inputStyle} /></label>
      </div>
      <div className="flex gap-2">
        <button disabled={!ready} onClick={submit} className="px-3 py-1.5 rounded-lg text-sm text-white" style={{ backgroundColor: 'var(--accent)', opacity: ready ? 1 : 0.5 }}>Registrar</button>
        <button onClick={onCancel} className="px-3 py-1.5 rounded-lg text-sm" style={inputStyle}>Cancelar</button>
      </div>
    </div>
  )
}

/** Ingresos por servicios registrados por proyecto */
export default function IncomesPanel({ projectId, projects, staff }: FinanceTabProps) {
  const { addToast } = useToast()
  const [list, setList] = useState<Income[] | null>(null)
  const [adding, setAdding] = useState(false)
  const load = useCallback(() => {
    fetchIncomes(projectId).then(setList).catch((e) => addToast(errorMessage(e), 'error'))
  }, [projectId, addToast])
  useEffect(load, [load])
  useEvent('finance', load, load)

  if (!list) return <div className="flex justify-center py-10"><Loader2 className="animate-spin" /></div>
  const title = (id: string) => projects.find((p) => p.id === id)?.title ?? id
  const total = list.filter((i) => !i.annulled_at).reduce((s, i) => s + i.amount, 0)

  const annul = async (id: string) => {
    const reason = window.prompt('Motivo de la anulación (queda en la bitácora):')
    if (!reason?.trim()) return
    try {
      await annulIncome(id, reason.trim())
      addToast('Ingreso anulado', 'success')
      load()
    } catch (e) {
      addToast(errorMessage(e), 'error')
    }
  }

  return (
    <div className="space-y-4">
      {staff && projectId && !adding && (
        <button onClick={() => setAdding(true)} className="flex items-center gap-1 px-3 py-1.5 rounded-lg text-sm text-white" style={{ backgroundColor: 'var(--accent)' }}><Plus size={14} /> Nuevo ingreso</button>
      )}
      {staff && !projectId && <p className="text-xs" style={lab}>Elige un proyecto arriba para registrar un ingreso.</p>}
      {adding && projectId && <IncomeForm projectId={projectId} onDone={() => { setAdding(false); load() }} onCancel={() => setAdding(false)} />}
      <div className="rounded-xl overflow-x-auto" style={cardStyle}>
        <table className="w-full text-sm" style={{ color: 'var(--text-primary)' }}>
          <thead><tr style={{ borderBottom: '1px solid var(--border)', color: 'var(--text-secondary)' }}>
            <th className={TH}>Fecha</th>{!projectId && <th className={TH}>Proyecto</th>}<th className={TH}>Origen</th><th className={TH}>Destino</th><th className={TH}>Monto</th><th className={TH}>Respaldo</th><th className={TH}>Registró</th><th className={TH}></th>
          </tr></thead>
          <tbody>
            {list.length === 0 && <tr><td colSpan={8} className="py-6 text-center text-xs" style={lab}>Sin ingresos registrados.</td></tr>}
            {list.map((i) => (
              <tr key={i.id} style={{ borderBottom: '1px solid var(--border)', opacity: i.annulled_at ? 0.55 : 1 }}>
                <td className="py-2 px-2 whitespace-nowrap">{fmtDay(i.received_on)}</td>
                {!projectId && <td className="px-2 text-xs">{title(i.project_id)}</td>}
                <td className="px-2">{i.origin}{i.notes && <div className="text-xs" style={lab}>{i.notes}</div>}{i.annulled_at && <div className="text-xs" style={{ color: 'var(--danger)' }}>Anulado por {i.annulled_by}: {i.annul_reason}</div>}</td>
                <td className="px-2 text-xs">{i.destination || '—'}</td>
                <td className="px-2 font-mono" style={{ textDecoration: i.annulled_at ? 'line-through' : undefined }}>{money(i.amount)}</td>
                <td className="px-2 text-xs">{i.file ? <a href={finFileUrl(i.file.id)} target="_blank" rel="noopener noreferrer" className="underline" title={`SHA-256 ${i.file.sha256}`}>{i.file.name}</a> : '—'}</td>
                <td className="px-2 text-xs">{i.created_by}{i.ledger_seq != null && <div style={lab}>bitácora #{i.ledger_seq}</div>}</td>
                <td className="px-2">{staff && !i.annulled_at && <button onClick={() => annul(i.id)} className="text-xs underline" style={{ color: 'var(--danger)' }}>Anular</button>}</td>
              </tr>
            ))}
          </tbody>
          {list.length > 0 && <tfoot><tr><td colSpan={projectId ? 3 : 4} className="py-2 px-2 text-right text-xs font-semibold">Total vigente</td><td className="px-2 font-mono font-semibold">{money(total)}</td><td colSpan={3} /></tr></tfoot>}
        </table>
      </div>
    </div>
  )
}

import { useCallback, useEffect, useState } from 'react'
import { Loader2 } from 'lucide-react'
import {
  annulPettyMove, createFund, createPettyCount, createPettyMove, fetchExecution, fetchFundDetail, fetchFunds, finFileUrl, fileToDataUrl,
  type Fund, type FundDetail, type ItemExec,
} from '../../../api'
import { useAuth } from '../../../auth/AuthContext'
import { useEvent } from '../../../events/useEvents'
import { useToast } from '../../ToastContext'
import { errorMessage } from '../../../lib/errors'
import { money, type FinanceTabProps } from '../../../lib/finance'
import { cardStyle, fmtDay, inputStyle, todayYmd } from '../../../lib/financeBudget'

const TH = 'text-left py-2 px-2 font-medium text-xs'
const field = 'block w-full mt-1 px-2 py-1.5 rounded-lg text-sm outline-none'
const lab = { color: 'var(--text-secondary)' }

function NewFund({ projectId, onDone }: { projectId: string; onDone: () => void }) {
  const { addToast } = useToast()
  const [name, setName] = useState('Caja chica')
  const [responsible, setResponsible] = useState('')
  const [days, setDays] = useState('30')
  const submit = async () => {
    try {
      await createFund({ project_id: projectId, name, responsible, arqueo_days: Number(days) })
      addToast('Caja chica abierta', 'success')
      onDone()
    } catch (e) {
      addToast(errorMessage(e), 'error')
    }
  }
  return (
    <div className="rounded-xl p-4 space-y-3" style={cardStyle}>
      <h3 className="text-sm font-semibold" style={{ color: 'var(--text-primary)' }}>Abrir caja chica del proyecto</h3>
      <div className="grid gap-3 sm:grid-cols-3">
        <label className="text-xs" style={lab}>Nombre<input value={name} onChange={(e) => setName(e.target.value)} maxLength={80} className={field} style={inputStyle} /></label>
        <label className="text-xs" style={lab}>Responsable (usuario)<input value={responsible} onChange={(e) => setResponsible(e.target.value)} className={field} style={inputStyle} /></label>
        <label className="text-xs" style={lab}>Arqueo cada (días)<input type="number" min={7} max={365} value={days} onChange={(e) => setDays(e.target.value)} className={field} style={inputStyle} /></label>
      </div>
      <button disabled={!responsible.trim()} onClick={submit} className="px-3 py-1.5 rounded-lg text-sm text-white" style={{ backgroundColor: 'var(--accent)', opacity: responsible.trim() ? 1 : 0.5 }}>Abrir caja</button>
    </div>
  )
}

function MoveForm({ fund, items, kind, onDone, onCancel }: { fund: Fund; items: ItemExec[]; kind: 'ingreso' | 'egreso'; onDone: () => void; onCancel: () => void }) {
  const { addToast } = useToast()
  const [amount, setAmount] = useState('')
  const [concept, setConcept] = useState('')
  const [date, setDate] = useState(todayYmd())
  const [item, setItem] = useState(items[0]?.id ?? '')
  const [file, setFile] = useState<File | null>(null)
  const [busy, setBusy] = useState(false)
  const egreso = kind === 'egreso'
  const ready = Number(amount) > 0 && concept.trim() && (!egreso || (item && file))
  const submit = async () => {
    setBusy(true)
    try {
      await createPettyMove(fund.id, {
        kind, amount: Number(amount), concept, moved_on: date,
        ...(egreso ? { budget_item_id: item, file: await fileToDataUrl(file!), file_name: file!.name } : {}),
      })
      addToast(egreso ? 'Gasto registrado' : 'Fondo repuesto', 'success')
      onDone()
    } catch (e) {
      addToast(errorMessage(e), 'error')
    } finally {
      setBusy(false)
    }
  }
  return (
    <div className="rounded-xl p-4 space-y-3" style={cardStyle}>
      <h3 className="text-sm font-semibold" style={{ color: 'var(--text-primary)' }}>{egreso ? 'Registrar gasto de la caja' : 'Reponer el fondo'} (saldo {money(fund.balance)})</h3>
      <div className="grid gap-3 sm:grid-cols-3">
        <label className="text-xs" style={lab}>Monto<input type="number" min={1} value={amount} onChange={(e) => setAmount(e.target.value)} className={field} style={inputStyle} /></label>
        <label className="text-xs sm:col-span-1" style={lab}>Concepto<input value={concept} onChange={(e) => setConcept(e.target.value)} maxLength={200} className={field} style={inputStyle} /></label>
        <label className="text-xs" style={lab}>Fecha<input type="date" max={todayYmd()} value={date} onChange={(e) => setDate(e.target.value)} className={field} style={inputStyle} /></label>
      </div>
      {egreso && (
        <div className="grid gap-3 sm:grid-cols-2">
          <label className="text-xs" style={lab}>Se imputa al ítem
            <select value={item} onChange={(e) => setItem(e.target.value)} className={field} style={inputStyle}>
              {items.map((i) => <option key={i.id} value={i.id}>{i.name}</option>)}
            </select>
          </label>
          <label className="text-xs" style={lab}>Respaldo (boleta, factura o foto; PDF, JPG o PNG)
            <input type="file" accept="application/pdf,image/jpeg,image/png" onChange={(e) => setFile(e.target.files?.[0] ?? null)} className={field} style={inputStyle} />
          </label>
        </div>
      )}
      {egreso && items.length === 0 && <p className="text-xs" style={{ color: 'var(--danger)' }}>El proyecto no tiene ítems de presupuesto: crea uno en Ejecución.</p>}
      <div className="flex gap-2">
        <button disabled={busy || !ready} onClick={submit} className="px-3 py-1.5 rounded-lg text-sm text-white" style={{ backgroundColor: 'var(--accent)', opacity: busy || !ready ? 0.5 : 1 }}>Registrar</button>
        <button onClick={onCancel} className="px-3 py-1.5 rounded-lg text-sm" style={inputStyle}>Cancelar</button>
      </div>
    </div>
  )
}

function CountForm({ fund, onDone, onCancel }: { fund: Fund; onDone: () => void; onCancel: () => void }) {
  const { addToast } = useToast()
  const [counted, setCounted] = useState('')
  const [notes, setNotes] = useState('')
  const diff = counted === '' ? null : Number(counted) - fund.balance
  const submit = async () => {
    try {
      await createPettyCount(fund.id, { counted: Number(counted), notes })
      addToast(diff ? 'Arqueo registrado con diferencia: se avisó al equipo' : 'Arqueo registrado: cuadra', diff ? 'info' : 'success')
      onDone()
    } catch (e) {
      addToast(errorMessage(e), 'error')
    }
  }
  return (
    <div className="rounded-xl p-4 space-y-3" style={cardStyle}>
      <h3 className="text-sm font-semibold" style={{ color: 'var(--text-primary)' }}>Arqueo: cuenta el dinero que hay en la caja</h3>
      <p className="text-xs" style={lab}>Saldo calculado por el sistema: {money(fund.balance)}. Quien custodia la caja no puede hacer su propio arqueo.</p>
      <div className="grid gap-3 sm:grid-cols-2">
        <label className="text-xs" style={lab}>Dinero contado<input type="number" min={0} value={counted} onChange={(e) => setCounted(e.target.value)} className={field} style={inputStyle} /></label>
        <label className="text-xs" style={lab}>Observaciones {diff ? '(obligatorias: hay diferencia)' : ''}<input value={notes} onChange={(e) => setNotes(e.target.value)} maxLength={500} className={field} style={inputStyle} /></label>
      </div>
      {diff != null && <p className="text-sm font-mono" style={{ color: diff === 0 ? 'var(--success)' : 'var(--danger)' }}>Diferencia: {money(diff)}</p>}
      <div className="flex gap-2">
        <button disabled={counted === '' || (!!diff && !notes.trim())} onClick={submit} className="px-3 py-1.5 rounded-lg text-sm text-white" style={{ backgroundColor: 'var(--accent)', opacity: counted === '' || (!!diff && !notes.trim()) ? 0.5 : 1 }}>Registrar arqueo</button>
        <button onClick={onCancel} className="px-3 py-1.5 rounded-lg text-sm" style={inputStyle}>Cancelar</button>
      </div>
    </div>
  )
}

function FundView({ fundId, items, staff, canUse, closed, onChange }: { fundId: string; items: ItemExec[]; staff: boolean; canUse: boolean; closed: boolean; onChange: () => void }) {
  const { addToast } = useToast()
  const { user } = useAuth()
  const [d, setD] = useState<FundDetail | null>(null)
  const [form, setForm] = useState<'ingreso' | 'egreso' | 'arqueo' | null>(null)
  const load = useCallback(() => {
    fetchFundDetail(fundId).then(setD).catch((e) => addToast(errorMessage(e), 'error'))
  }, [fundId, addToast])
  useEffect(load, [load])
  useEvent('finance', load, load)
  if (!d) return <div className="flex justify-center py-6"><Loader2 className="animate-spin" /></div>
  const { fund, moves, counts } = d
  const mine = user?.username === fund.responsible
  const canSpend = !closed && canUse && (staff || mine)
  const done = () => { setForm(null); load(); onChange() }

  const annul = async (id: string) => {
    const reason = window.prompt('Motivo de la anulación (queda en la bitácora):')
    if (!reason?.trim()) return
    try {
      await annulPettyMove(id, reason.trim())
      addToast('Movimiento anulado', 'success')
      load()
      onChange()
    } catch (e) {
      addToast(errorMessage(e), 'error')
    }
  }

  return (
    <div className="space-y-4">
      <div className="rounded-xl p-4 flex flex-wrap items-center gap-x-6 gap-y-2 text-sm" style={{ ...cardStyle, color: 'var(--text-primary)' }}>
        <div><div className="text-xs" style={lab}>Saldo</div><div className="text-xl font-mono">{money(fund.balance)}</div></div>
        <div><div className="text-xs" style={lab}>Responsable</div>{fund.responsible}</div>
        <div>
          <div className="text-xs" style={lab}>Próximo arqueo</div>
          <span style={{ color: fund.arqueo_overdue ? 'var(--danger)' : undefined }}>{fmtDay(fund.next_arqueo)}{fund.arqueo_overdue && ' (vencido)'}</span>
        </div>
        {fund.last_difference != null && <div><div className="text-xs" style={lab}>Último arqueo</div><span style={{ color: fund.last_difference === 0 ? 'var(--success)' : 'var(--danger)' }}>{fund.last_difference === 0 ? 'cuadró' : `diferencia ${money(fund.last_difference)}`}</span></div>}
        {!closed && (
          <div className="ml-auto flex gap-2 flex-wrap">
            {canSpend && <button onClick={() => setForm('egreso')} className="px-3 py-1.5 rounded-lg text-sm text-white" style={{ backgroundColor: 'var(--accent)' }}>Registrar gasto</button>}
            {staff && <button onClick={() => setForm('ingreso')} className="px-3 py-1.5 rounded-lg text-sm" style={inputStyle}>Reponer fondo</button>}
            {staff && !mine && <button onClick={() => setForm('arqueo')} className="px-3 py-1.5 rounded-lg text-sm" style={inputStyle}>Hacer arqueo</button>}
          </div>
        )}
      </div>
      {form === 'arqueo' ? <CountForm fund={fund} onDone={done} onCancel={() => setForm(null)} />
        : form ? <MoveForm fund={fund} items={items} kind={form} onDone={done} onCancel={() => setForm(null)} /> : null}

      <div className="rounded-xl overflow-x-auto" style={cardStyle}>
        <table className="w-full text-sm" style={{ color: 'var(--text-primary)' }}>
          <thead><tr style={{ borderBottom: '1px solid var(--border)', color: 'var(--text-secondary)' }}>
            <th className={TH}>Fecha</th><th className={TH}>Concepto</th><th className={TH}>Ítem</th><th className={TH}>Monto</th><th className={TH}>Respaldo</th><th className={TH}>Registró</th><th className={TH}></th>
          </tr></thead>
          <tbody>
            {moves.length === 0 && <tr><td colSpan={7} className="py-6 text-center text-xs" style={lab}>Sin movimientos.</td></tr>}
            {moves.map((m) => (
              <tr key={m.id} style={{ borderBottom: '1px solid var(--border)', opacity: m.annulled_at ? 0.55 : 1 }}>
                <td className="py-2 px-2 whitespace-nowrap">{fmtDay(m.moved_on)}</td>
                <td className="px-2">{m.concept}{m.annulled_at && <div className="text-xs" style={{ color: 'var(--danger)' }}>Anulado por {m.annulled_by}: {m.annul_reason}</div>}</td>
                <td className="px-2 text-xs">{m.item_name ?? '—'}</td>
                <td className="px-2 font-mono" style={{ color: m.kind === 'ingreso' ? 'var(--success)' : undefined, textDecoration: m.annulled_at ? 'line-through' : undefined }}>
                  {m.kind === 'ingreso' ? '+' : '-'}{money(m.amount)}
                </td>
                <td className="px-2 text-xs">{m.file ? <a href={finFileUrl(m.file.id)} target="_blank" rel="noopener noreferrer" className="underline" title={`SHA-256 ${m.file.sha256}`}>{m.file.name}</a> : '—'}</td>
                <td className="px-2 text-xs">{m.created_by}{m.ledger_seq != null && <div style={lab}>bitácora #{m.ledger_seq}</div>}</td>
                <td className="px-2">{staff && !closed && !m.annulled_at && <button onClick={() => annul(m.id)} className="text-xs underline" style={{ color: 'var(--danger)' }}>Anular</button>}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      {counts.length > 0 && (
        <div className="rounded-xl p-4 space-y-1" style={cardStyle}>
          <h3 className="text-sm font-semibold" style={{ color: 'var(--text-primary)' }}>Arqueos</h3>
          {counts.map((c) => (
            <div key={c.id} className="text-xs" style={{ color: 'var(--text-primary)' }}>
              {fmtDay(c.counted_at)} · {c.counted_by}: contado {money(c.counted)} de {money(c.expected)} calculado →{' '}
              <span style={{ color: c.difference === 0 ? 'var(--success)' : 'var(--danger)' }}>{c.difference === 0 ? 'cuadra' : `diferencia ${money(c.difference)}`}</span>
              {c.notes && ` — ${c.notes}`}{c.ledger_seq != null && ` (bitácora #${c.ledger_seq})`}
            </div>
          ))}
        </div>
      )}
    </div>
  )
}

/** Caja chica por proyecto: fondo, gastos con respaldo y arqueo */
export default function PettyPanel({ projectId, staff, canUse }: FinanceTabProps) {
  const { addToast } = useToast()
  const [funds, setFunds] = useState<Fund[] | null>(null)
  const [items, setItems] = useState<ItemExec[]>([])
  const [closed, setClosed] = useState(false)
  const load = useCallback(() => {
    fetchFunds(projectId).then(setFunds).catch((e) => addToast(errorMessage(e), 'error'))
    if (projectId) fetchExecution(projectId).then((x) => { setItems(x.items); setClosed(x.config.closed_at != null) }).catch(() => undefined)
  }, [projectId, addToast])
  useEffect(load, [load])
  useEvent('finance', load, load)

  if (!funds) return <div className="flex justify-center py-10"><Loader2 className="animate-spin" /></div>
  if (!projectId) {
    if (funds.length === 0) return <p className="text-sm text-center py-10" style={lab}>Ningún proyecto tiene caja chica. Elige un proyecto arriba para abrir la suya.</p>
    return (
      <div className="rounded-xl overflow-x-auto" style={cardStyle}>
        <table className="w-full text-sm" style={{ color: 'var(--text-primary)' }}>
          <thead><tr style={{ borderBottom: '1px solid var(--border)', color: 'var(--text-secondary)' }}><th className={TH}>Proyecto</th><th className={TH}>Responsable</th><th className={TH}>Saldo</th><th className={TH}>Próximo arqueo</th></tr></thead>
          <tbody>
            {funds.map((f) => (
              <tr key={f.id} style={{ borderBottom: '1px solid var(--border)' }}>
                <td className="py-2 px-2">{f.title}</td><td className="px-2">{f.responsible}</td><td className="px-2 font-mono">{money(f.balance)}</td>
                <td className="px-2" style={{ color: f.arqueo_overdue ? 'var(--danger)' : undefined }}>{fmtDay(f.next_arqueo)}{f.arqueo_overdue && ' (vencido)'}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    )
  }
  if (funds.length === 0) {
    return staff && !closed ? <NewFund projectId={projectId} onDone={load} /> : <p className="text-sm text-center py-10" style={lab}>Este proyecto no tiene caja chica.</p>
  }
  return <FundView fundId={funds[0].id} items={items} staff={staff} canUse={canUse} closed={closed} onChange={load} />
}

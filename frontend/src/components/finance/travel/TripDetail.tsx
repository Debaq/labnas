import { useState } from 'react'
import { Download, Loader2, Trash2 } from 'lucide-react'
import {
  addTripReceipt, approveTrip, cancelTrip, closeTrip, fileToDataUrl, finFileUrl, rejectTrip, removeTripReceipt, saveTripReport,
  type ReceiptKind, type Trip,
} from '../../../api'
import { useAuth } from '../../../auth/AuthContext'
import { useToast } from '../../ToastContext'
import { errorMessage } from '../../../lib/errors'
import { money } from '../../../lib/finance'
import { MIN_TRIP_REPORT, RECEIPT_KIND, TRIP_STATUS } from '../../../lib/financeTravel'
import { cardStyle, fmtDay, inputStyle } from '../../../lib/transparency'

const field = 'block w-full mt-1 px-2 py-1.5 rounded-lg text-sm outline-none'
const label = { color: 'var(--text-secondary)' }
const btn = 'px-3 py-1.5 rounded-lg text-xs font-medium disabled:opacity-50'

/** Ficha del viaje: decision, comprobantes con respaldo, informe de comision y cierre */
export default function TripDetail({ trip, staff, canUse, onChange }: { trip: Trip; staff: boolean; canUse: boolean; onChange: (t: Trip) => void }) {
  const { user } = useAuth()
  const { addToast } = useToast()
  const me = user?.username ?? ''
  const involved = [trip.requested_by, trip.traveler_user, trip.traveler].some((u) => u && u.toLowerCase() === me.toLowerCase())
  const canDecide = staff && !involved && trip.status === 'solicitud'
  const canWork = (involved || staff) && canUse
  const [note, setNote] = useState('')
  const [report, setReport] = useState(trip.report_text)
  const [kind, setKind] = useState<ReceiptKind>('pasaje')
  const [desc, setDesc] = useState('')
  const [amount, setAmount] = useState('')
  const [file, setFile] = useState<File | null>(null)
  const [busy, setBusy] = useState(false)
  const st = TRIP_STATUS[trip.status]

  const run = async (fn: () => Promise<Trip>, ok: string) => {
    setBusy(true)
    try { const t = await fn(); onChange(t); addToast(ok, 'success'); return t } catch (e) { addToast(errorMessage(e), 'error') } finally { setBusy(false) }
  }

  const addReceipt = async () => {
    const data = file ? { name: file.name, data: await fileToDataUrl(file) } : null
    const t = await run(() => addTripReceipt(trip.id, { kind, description: desc, amount: kind === 'informe' ? 0 : Number(amount), file: data }), 'Comprobante agregado')
    if (t) { setDesc(''); setAmount(''); setFile(null) }
  }

  const receipts = trip.receipts ?? []
  const open = trip.status === 'aprobado'

  return (
    <div className="rounded-xl p-4 space-y-4" style={cardStyle}>
      <div className="flex flex-wrap items-center gap-2">
        <h2 className="text-base font-semibold" style={{ color: 'var(--text-primary)' }}>V-{trip.folio} · {trip.destination}</h2>
        <span className="text-[11px] px-2 py-0.5 rounded-full" style={{ color: st.color, border: '1px solid currentColor' }}>{st.label}</span>
      </div>
      <div className="grid gap-x-6 gap-y-1 text-sm sm:grid-cols-2" style={{ color: 'var(--text-primary)' }}>
        <p><span className="text-xs" style={label}>Proyecto: </span>{trip.project_title}</p>
        <p><span className="text-xs" style={label}>Viaja: </span>{trip.traveler}</p>
        <p><span className="text-xs" style={label}>Fechas: </span>{fmtDay(trip.start_date)} al {fmtDay(trip.end_date)}</p>
        <p><span className="text-xs" style={label}>Pidio: </span>{trip.requested_by}</p>
        <p className="sm:col-span-2 whitespace-pre-wrap"><span className="text-xs" style={label}>Motivo: </span>{trip.purpose}</p>
        <p><span className="text-xs" style={label}>Estimado: </span>pasajes {money(trip.est_transport)} · alojamiento {money(trip.est_lodging)} · viatico {money(trip.est_per_diem)} = <strong>{money(trip.est_total)}</strong></p>
        <p><span className="text-xs" style={label}>{trip.status === 'cerrado' ? 'Gastado: ' : 'Comprobantes: '}</span><strong>{money(trip.spent)}</strong></p>
        {trip.decided_by && <p><span className="text-xs" style={label}>{trip.status === 'rechazado' ? 'Rechazado por: ' : 'Decidido por: '}</span>{trip.decided_by}{trip.decision_note ? ` · ${trip.decision_note}` : ''}</p>}
      </div>

      {trip.status === 'solicitud' && (
        <div className="space-y-2">
          {canDecide ? (
            <>
              <input value={note} onChange={(e) => setNote(e.target.value)} maxLength={1000} placeholder="Nota (obligatoria para rechazar)" className={field} style={inputStyle} />
              <div className="flex gap-2">
                <button disabled={busy} onClick={() => run(() => approveTrip(trip.id, note), 'Viaje aprobado: el estimado quedo comprometido')} className={btn} style={{ backgroundColor: 'var(--accent)', color: '#fff' }}>Aprobar</button>
                <button disabled={busy || !note.trim()} onClick={() => run(() => rejectTrip(trip.id, note), 'Viaje rechazado')} className={btn} style={{ border: '1px solid var(--danger)', color: 'var(--danger)' }}>Rechazar</button>
              </div>
            </>
          ) : (
            <p className="text-xs" style={label}>{staff && involved ? 'Pediste o viajas en este viaje: lo debe aprobar otra persona que gestione Finanzas.' : 'Esperando la aprobacion de quien gestiona Finanzas.'}</p>
          )}
        </div>
      )}

      {(open || receipts.length > 0) && (
        <div className="space-y-2">
          <h3 className="text-sm font-semibold" style={{ color: 'var(--text-primary)' }}>Comprobantes</h3>
          <ul className="divide-y" style={{ borderColor: 'var(--card-border)' }}>
            {receipts.map((r) => (
              <li key={r.id} className="py-2 flex items-center gap-3 text-sm" style={{ color: 'var(--text-primary)' }}>
                <span className="flex-1 min-w-0">
                  {RECEIPT_KIND[r.kind]}{r.description ? ` · ${r.description}` : ''}
                  {r.file && <span className="block text-[11px] font-mono truncate" style={label} title={r.file.sha256}>SHA-256 {r.file.sha256.slice(0, 16)}...</span>}
                </span>
                {r.kind !== 'informe' && <span>{money(r.amount)}</span>}
                {r.file && <a href={finFileUrl(r.file.id)} target="_blank" rel="noreferrer" style={{ color: 'var(--accent)' }} title={r.file.name}><Download size={15} /></a>}
                {open && canWork && <button disabled={busy} onClick={() => run(() => removeTripReceipt(trip.id, r.id), 'Comprobante quitado')} style={{ color: 'var(--danger)' }} title="Quitar"><Trash2 size={15} /></button>}
              </li>
            ))}
            {receipts.length === 0 && <li className="py-2 text-xs" style={label}>Todavia no hay comprobantes.</li>}
          </ul>
          {open && canWork && (
            <div className="grid gap-2 sm:grid-cols-4 items-end">
              <label className="text-xs" style={label}>Tipo
                <select value={kind} onChange={(e) => setKind(e.target.value as ReceiptKind)} className={field} style={inputStyle}>
                  {(Object.keys(RECEIPT_KIND) as ReceiptKind[]).map((k) => <option key={k} value={k}>{RECEIPT_KIND[k]}</option>)}
                </select>
              </label>
              <label className="text-xs" style={label}>Descripcion
                <input value={desc} maxLength={300} onChange={(e) => setDesc(e.target.value)} className={field} style={inputStyle} />
              </label>
              {kind !== 'informe' && (
                <label className="text-xs" style={label}>Monto ($)
                  <input type="number" min={0} value={amount} onChange={(e) => setAmount(e.target.value)} className={field} style={inputStyle} />
                </label>
              )}
              <label className="text-xs" style={label}>Respaldo {kind === 'viatico' ? '(opcional)' : ''}
                <input type="file" accept=".pdf,.png,.jpg,.jpeg,.xml,.xlsx,.docx" onChange={(e) => setFile(e.target.files?.[0] ?? null)} className="block w-full mt-1 text-xs" style={{ color: 'var(--text-primary)' }} />
              </label>
              <button disabled={busy || (kind !== 'informe' && !Number(amount)) || (kind !== 'viatico' && !file)} onClick={addReceipt} className={btn} style={{ backgroundColor: 'var(--accent)', color: '#fff' }}>Agregar</button>
            </div>
          )}
        </div>
      )}

      {(open || trip.report_text) && (
        <div className="space-y-2">
          <h3 className="text-sm font-semibold" style={{ color: 'var(--text-primary)' }}>Informe de comision</h3>
          {open && canWork ? (
            <>
              <textarea rows={5} maxLength={10000} value={report} onChange={(e) => setReport(e.target.value)} placeholder="Que se hizo, con quien, resultados. Los archivos del informe se agregan arriba como «Adjunto del informe»." className={field} style={inputStyle} />
              <div className="flex flex-wrap gap-2 items-center">
                <button disabled={busy || report === trip.report_text} onClick={() => run(() => saveTripReport(trip.id, report), 'Informe guardado')} className={btn} style={{ border: '1px solid var(--border)', color: 'var(--text-primary)' }}>Guardar informe</button>
                <button disabled={busy || trip.report_text.trim().length < MIN_TRIP_REPORT || report !== trip.report_text} onClick={() => run(() => closeTrip(trip.id), 'Viaje cerrado')} className={btn} style={{ backgroundColor: 'var(--accent)', color: '#fff' }}>
                  {busy ? <Loader2 size={12} className="animate-spin inline" /> : null} Cerrar viaje ({money(trip.spent)})
                </button>
                <span className="text-xs" style={label}>Para cerrar hace falta el informe guardado (minimo {MIN_TRIP_REPORT} caracteres).</span>
              </div>
            </>
          ) : (
            <p className="text-sm whitespace-pre-wrap" style={{ color: 'var(--text-primary)' }}>{trip.report_text}</p>
          )}
        </div>
      )}

      {(trip.status === 'solicitud' || trip.status === 'aprobado') && canWork && (
        <button disabled={busy} onClick={() => { const r = window.prompt('Motivo de la anulacion'); if (r?.trim()) run(() => cancelTrip(trip.id, r.trim()), 'Viaje anulado') }} className={btn} style={{ border: '1px solid var(--danger)', color: 'var(--danger)' }}>Anular viaje</button>
      )}

      {trip.steps && trip.steps.length > 0 && (
        <div>
          <p className="text-xs font-medium mb-1" style={label}>Historial (aprobaciones y cierre quedan en la bitacora)</p>
          <ul className="space-y-0.5">
            {trip.steps.map((s, i) => (
              <li key={i} className="text-xs" style={label}>{fmtDay(s.at)} {s.at.slice(11, 16)} · {TRIP_STATUS[s.step as keyof typeof TRIP_STATUS]?.label ?? s.step} · {s.username}{s.note ? ` · ${s.note}` : ''}{s.ledger_seq ? ` · bitacora #${s.ledger_seq}` : ''}</li>
            ))}
          </ul>
        </div>
      )}
    </div>
  )
}

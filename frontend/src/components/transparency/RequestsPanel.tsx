import { useCallback, useEffect, useState } from 'react'
import { ChevronDown, ChevronRight, Loader2 } from 'lucide-react'
import { fetchInfoRequest, fetchInfoRequests, updateInfoRequest, type InfoRequest, type InfoRequestEvent, type RequestStatus } from '../../api'
import { useEvent } from '../../events/useEvents'
import { useToast } from '../ToastContext'
import { errorMessage } from '../../lib/errors'
import { REQUEST_STATUS, cardStyle, deadlineColor, deadlineLabel, fmtDay, inputStyle } from '../../lib/transparency'

const FILTERS: { id: string; label: string }[] = [
  { id: '', label: 'Todas' },
  { id: 'recibida', label: 'Recibidas' },
  { id: 'en_curso', label: 'En curso' },
  { id: 'respondida', label: 'Respondidas' },
  { id: 'derivada', label: 'Derivadas' },
  { id: 'rechazada', label: 'Rechazadas' },
]

function Chip({ label, color }: { label: string; color: string }) {
  return <span className="text-[11px] px-2 py-0.5 rounded-full whitespace-nowrap" style={{ color, border: '1px solid currentColor' }}>{label}</span>
}

function Detail({ req, managers, onChanged }: { req: InfoRequest; managers: string[]; onChanged: () => void }) {
  const { addToast } = useToast()
  const [events, setEvents] = useState<InfoRequestEvent[]>([])
  const [assignee, setAssignee] = useState(req.assigned_to ?? '')
  const [response, setResponse] = useState(req.response)
  const [reason, setReason] = useState(req.reason)
  const [derived, setDerived] = useState(req.derived_to)
  const [note, setNote] = useState('')
  const [busy, setBusy] = useState(false)
  const open = req.status === 'recibida' || req.status === 'en_curso'

  useEffect(() => { fetchInfoRequest(req.id).then((d) => setEvents(d.events)).catch(() => setEvents([])) }, [req.id, req.status])

  const act = async (status: RequestStatus | undefined, ok: string) => {
    setBusy(true)
    try {
      await updateInfoRequest(req.id, { status, assigned_to: assignee, response, reason, derived_to: derived, note })
      setNote('')
      addToast(ok, 'success')
      onChanged()
    } catch (e) {
      addToast(errorMessage(e), 'error')
    } finally {
      setBusy(false)
    }
  }

  const btn = 'px-3 py-1.5 rounded-lg text-xs font-medium disabled:opacity-50'
  return (
    <div className="px-4 pb-4 space-y-3">
      <p className="text-sm whitespace-pre-wrap" style={{ color: 'var(--text-primary)' }}>{req.body}</p>
      <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>{req.name}{req.email ? ` · ${req.email}` : ' · sin correo'} · recibida el {fmtDay(req.created_at)}</p>
      {open ? (
        <div className="space-y-3">
          <label className="block text-xs" style={{ color: 'var(--text-secondary)' }}>Responsable
            <select value={assignee} onChange={(e) => setAssignee(e.target.value)} className="block w-full mt-1 px-2 py-1.5 rounded-lg text-sm outline-none" style={inputStyle}>
              <option value="">Sin asignar</option>
              {managers.map((m) => <option key={m} value={m}>{m}</option>)}
            </select>
          </label>
          <label className="block text-xs" style={{ color: 'var(--text-secondary)' }}>Respuesta (la ve quien pregunto con su folio y codigo)
            <textarea rows={4} maxLength={8000} value={response} onChange={(e) => setResponse(e.target.value)} className="block w-full mt-1 px-2 py-1.5 rounded-lg text-sm outline-none" style={inputStyle} />
          </label>
          <div className="grid gap-3 sm:grid-cols-2">
            <label className="block text-xs" style={{ color: 'var(--text-secondary)' }}>Motivo del rechazo
              <input maxLength={1000} value={reason} onChange={(e) => setReason(e.target.value)} className="block w-full mt-1 px-2 py-1.5 rounded-lg text-sm outline-none" style={inputStyle} />
            </label>
            <label className="block text-xs" style={{ color: 'var(--text-secondary)' }}>Derivada a (otra oficina o institucion)
              <input maxLength={200} value={derived} onChange={(e) => setDerived(e.target.value)} className="block w-full mt-1 px-2 py-1.5 rounded-lg text-sm outline-none" style={inputStyle} />
            </label>
          </div>
          <label className="block text-xs" style={{ color: 'var(--text-secondary)' }}>Nota interna (no se publica)
            <input maxLength={1000} value={note} onChange={(e) => setNote(e.target.value)} className="block w-full mt-1 px-2 py-1.5 rounded-lg text-sm outline-none" style={inputStyle} />
          </label>
          <div className="flex flex-wrap gap-2">
            <button disabled={busy} onClick={() => act('en_curso', 'Solicitud en curso')} className={btn} style={{ border: '1px solid var(--border)', color: 'var(--text-primary)' }}>Guardar como en curso</button>
            <button disabled={busy} onClick={() => act('respondida', 'Solicitud respondida')} className={btn} style={{ backgroundColor: 'var(--accent)', color: '#fff' }}>Responder</button>
            <button disabled={busy} onClick={() => act('derivada', 'Solicitud derivada')} className={btn} style={{ border: '1px solid var(--border)', color: 'var(--text-primary)' }}>Derivar</button>
            <button disabled={busy} onClick={() => act('rechazada', 'Solicitud rechazada')} className={btn} style={{ border: '1px solid var(--danger)', color: 'var(--danger)' }}>Rechazar</button>
          </div>
        </div>
      ) : (
        <div className="text-sm space-y-1" style={{ color: 'var(--text-primary)' }}>
          {req.response && <p className="whitespace-pre-wrap"><span className="text-xs" style={{ color: 'var(--text-secondary)' }}>Respuesta: </span>{req.response}</p>}
          {req.reason && <p><span className="text-xs" style={{ color: 'var(--text-secondary)' }}>Motivo: </span>{req.reason}</p>}
          {req.derived_to && <p><span className="text-xs" style={{ color: 'var(--text-secondary)' }}>Derivada a: </span>{req.derived_to}</p>}
        </div>
      )}
      <div>
        <p className="text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Historial (cada cambio queda en la bitacora)</p>
        <ul className="space-y-0.5">
          {events.map((e, i) => (
            <li key={i} className="text-xs" style={{ color: 'var(--text-secondary)' }}>
              {fmtDay(e.at)} {e.at.slice(11, 16)} · {REQUEST_STATUS[e.status]?.label ?? e.status} · {e.username}{e.note ? ` · ${e.note}` : ''}{e.ledger_seq ? ` · bitacora #${e.ledger_seq}` : ''}
            </li>
          ))}
        </ul>
      </div>
    </div>
  )
}

/** Solicitudes de informacion: lista con plazo en colores y gestion */
export default function RequestsPanel({ managers }: { managers: string[] }) {
  const { addToast } = useToast()
  const [filter, setFilter] = useState('')
  const [rows, setRows] = useState<InfoRequest[] | null>(null)
  const [openId, setOpenId] = useState<string | null>(null)

  const load = useCallback(() => {
    fetchInfoRequests(filter || undefined).then(setRows).catch((e) => { setRows([]); addToast(errorMessage(e), 'error') })
  }, [filter, addToast])
  useEffect(load, [load])
  useEvent('transparency', load)

  return (
    <div className="space-y-3">
      <div className="flex flex-wrap gap-2">
        {FILTERS.map((f) => (
          <button key={f.id} onClick={() => setFilter(f.id)} className="px-3 py-1 rounded-full text-xs"
            style={{ backgroundColor: filter === f.id ? 'var(--accent)' : 'transparent', color: filter === f.id ? '#fff' : 'var(--text-secondary)', border: '1px solid var(--border)' }}>{f.label}</button>
        ))}
      </div>
      <div className="rounded-xl overflow-hidden" style={cardStyle}>
        {rows == null && <div className="p-4"><Loader2 className="animate-spin" size={18} style={{ color: 'var(--accent)' }} /></div>}
        {rows?.length === 0 && <p className="p-4 text-sm" style={{ color: 'var(--text-secondary)' }}>No hay solicitudes.</p>}
        <ul className="divide-y" style={{ borderColor: 'var(--card-border)' }}>
          {rows?.map((r) => {
            const isOpen = openId === r.id
            const st = REQUEST_STATUS[r.status]
            return (
              <li key={r.id}>
                <button onClick={() => setOpenId(isOpen ? null : r.id)} className="w-full text-left px-4 py-3 flex items-center gap-3">
                  {isOpen ? <ChevronDown size={14} style={{ color: 'var(--text-secondary)' }} /> : <ChevronRight size={14} style={{ color: 'var(--text-secondary)' }} />}
                  <span className="font-mono text-xs" style={{ color: 'var(--text-secondary)' }}>{r.folio}</span>
                  <span className="flex-1 min-w-0 truncate text-sm" style={{ color: 'var(--text-primary)' }}>{r.body}</span>
                  {r.assigned_to && <span className="text-xs hidden sm:inline" style={{ color: 'var(--text-secondary)' }}>{r.assigned_to}</span>}
                  {r.days_left != null && <Chip label={deadlineLabel(r.days_left)} color={deadlineColor(r.days_left)} />}
                  <Chip label={st.label} color={st.color} />
                </button>
                {isOpen && <Detail req={r} managers={managers} onChanged={load} />}
              </li>
            )
          })}
        </ul>
      </div>
    </div>
  )
}

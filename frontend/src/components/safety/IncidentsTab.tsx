import { useEffect, useState } from 'react'
import { Camera, Loader2, Plus } from 'lucide-react'
import {
  fetchMachines,
  incidentPhotoUrl,
  reportIncident,
  updateIncident,
  type IncidentSeverity,
  type IncidentStatus,
  type Machine,
  type SafetyIncident,
} from '../../api'
import { useToast } from '../ToastContext'
import { errorMessage } from '../../lib/errors'
import { INCIDENT_STATUS, SEVERITY, cardStyle, fmtDateTime, inputStyle, nowLocalInput, readAsDataUrl } from '../../lib/safety'

function Chip({ label, color }: { label: string; color: string }) {
  return <span className="text-[11px] px-2 py-0.5 rounded-full" style={{ color, border: '1px solid currentColor' }}>{label}</span>
}

function ReportForm({ machines, onDone }: { machines: Machine[]; onDone: () => void }) {
  const { addToast } = useToast()
  const [severity, setSeverity] = useState<IncidentSeverity>('casi_accidente')
  const [description, setDescription] = useState('')
  const [place, setPlace] = useState('')
  const [when, setWhen] = useState(nowLocalInput())
  const [machine, setMachine] = useState('')
  const [actions, setActions] = useState('')
  const [photo, setPhoto] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  const submit = async () => {
    setBusy(true)
    try {
      await reportIncident({ severity, description, place, occurred_at: when || null, machine_id: machine || null, photo, actions })
      addToast(severity === 'grave' ? 'Incidente grave reportado: se aviso a quienes gestionan Seguridad' : 'Incidente reportado', 'success')
      onDone()
    } catch (e) {
      addToast(errorMessage(e), 'error')
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="rounded-xl p-4 space-y-3" style={cardStyle}>
      <h2 className="text-sm font-semibold" style={{ color: 'var(--text-primary)' }}>Reportar incidente o casi accidente</h2>
      <div className="grid gap-3 sm:grid-cols-2">
        <label className="text-xs" style={{ color: 'var(--text-secondary)' }}>Gravedad
          <select value={severity} onChange={(e) => setSeverity(e.target.value as IncidentSeverity)} className="block w-full mt-1 px-2 py-1.5 rounded-lg text-sm outline-none" style={inputStyle}>
            {(Object.keys(SEVERITY) as IncidentSeverity[]).map((k) => <option key={k} value={k}>{SEVERITY[k].label}</option>)}
          </select>
        </label>
        <label className="text-xs" style={{ color: 'var(--text-secondary)' }}>Cuando ocurrio
          <input type="datetime-local" value={when} onChange={(e) => setWhen(e.target.value)} className="block w-full mt-1 px-2 py-1.5 rounded-lg text-sm outline-none" style={inputStyle} />
        </label>
        <label className="text-xs" style={{ color: 'var(--text-secondary)' }}>Donde
          <input value={place} onChange={(e) => setPlace(e.target.value)} maxLength={200} placeholder="Sala o zona" className="block w-full mt-1 px-2 py-1.5 rounded-lg text-sm outline-none" style={inputStyle} />
        </label>
        <label className="text-xs" style={{ color: 'var(--text-secondary)' }}>Maquina (opcional)
          <select value={machine} onChange={(e) => setMachine(e.target.value)} className="block w-full mt-1 px-2 py-1.5 rounded-lg text-sm outline-none" style={inputStyle}>
            <option value="">Ninguna</option>
            {machines.map((m) => <option key={m.id} value={m.id}>{m.name}</option>)}
          </select>
        </label>
      </div>
      <label className="text-xs block" style={{ color: 'var(--text-secondary)' }}>Que paso
        <textarea value={description} onChange={(e) => setDescription(e.target.value)} maxLength={4000} rows={3} className="block w-full mt-1 px-3 py-2 rounded-lg text-sm outline-none" style={inputStyle} />
      </label>
      <label className="text-xs block" style={{ color: 'var(--text-secondary)' }}>Que se hizo en el momento (opcional)
        <textarea value={actions} onChange={(e) => setActions(e.target.value)} maxLength={4000} rows={2} className="block w-full mt-1 px-3 py-2 rounded-lg text-sm outline-none" style={inputStyle} />
      </label>
      <div className="flex items-center gap-3 flex-wrap">
        <label className="flex items-center gap-1.5 text-xs px-3 py-1.5 rounded-lg cursor-pointer" style={inputStyle}>
          <Camera size={14} /> {photo ? 'Cambiar foto' : 'Agregar foto'}
          <input type="file" accept="image/jpeg,image/png" className="hidden" onChange={async (e) => {
            const f = e.target.files?.[0]
            e.target.value = ''
            if (!f) return
            if (f.size > 2 * 1024 * 1024) { addToast('La foto pesa mas de 2 MB', 'error'); return }
            setPhoto(await readAsDataUrl(f))
          }} />
        </label>
        {photo && <img src={photo} alt="" className="h-12 rounded" />}
        <button disabled={busy || !description.trim()} onClick={submit} className="ml-auto flex items-center gap-1.5 px-4 py-2 rounded-lg text-sm font-medium"
          style={{ backgroundColor: 'var(--accent)', color: '#fff', opacity: !description.trim() ? 0.6 : 1 }}>
          {busy && <Loader2 size={14} className="animate-spin" />} Reportar
        </button>
      </div>
    </div>
  )
}

function IncidentCard({ inc, staff, onChange }: { inc: SafetyIncident; staff: boolean; onChange: () => void }) {
  const { addToast } = useToast()
  const [open, setOpen] = useState(false)
  const [status, setStatus] = useState<IncidentStatus>(inc.status)
  const [actions, setActions] = useState(inc.actions)
  const [note, setNote] = useState(inc.closing_note)
  const [busy, setBusy] = useState(false)
  const sev = SEVERITY[inc.severity]
  const st = INCIDENT_STATUS[inc.status]

  const save = async () => {
    setBusy(true)
    try {
      await updateIncident(inc.id, { status, actions, closing_note: note })
      addToast('Incidente actualizado', 'success')
      onChange()
    } catch (e) {
      addToast(errorMessage(e), 'error')
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="rounded-xl p-4 space-y-2" style={cardStyle}>
      <div className="flex items-center gap-2 flex-wrap">
        <Chip label={sev.label} color={sev.color} />
        <Chip label={st.label} color={st.color} />
        <span className="text-xs" style={{ color: 'var(--text-secondary)' }}>
          {fmtDateTime(inc.occurred_at)}{inc.place && ` · ${inc.place}`}{inc.machine_name && ` · ${inc.machine_name}`} · reporta {inc.reported_by}
        </span>
        {staff && (
          <button onClick={() => setOpen(!open)} className="ml-auto text-xs" style={{ color: 'var(--accent)' }}>{open ? 'Cerrar panel' : 'Revisar'}</button>
        )}
      </div>
      <p className="text-sm whitespace-pre-wrap" style={{ color: 'var(--text-primary)' }}>{inc.description}</p>
      {inc.has_photo && <img src={incidentPhotoUrl(inc.id)} alt="Foto del incidente" className="max-h-48 rounded-lg" loading="lazy" />}
      {inc.actions && <p className="text-xs whitespace-pre-wrap" style={{ color: 'var(--text-secondary)' }}><b>Acciones:</b> {inc.actions}</p>}
      {inc.status === 'cerrado' && (
        <p className="text-xs whitespace-pre-wrap" style={{ color: 'var(--text-secondary)' }}>
          <b>Cierre ({inc.closed_by}):</b> {inc.closing_note}
        </p>
      )}
      {staff && open && (
        <div className="space-y-2 pt-2" style={{ borderTop: '1px solid var(--border)' }}>
          <label className="text-xs block" style={{ color: 'var(--text-secondary)' }}>Estado
            <select value={status} onChange={(e) => setStatus(e.target.value as IncidentStatus)} className="block mt-1 px-2 py-1.5 rounded-lg text-sm outline-none" style={inputStyle}>
              {(Object.keys(INCIDENT_STATUS) as IncidentStatus[]).map((k) => <option key={k} value={k}>{INCIDENT_STATUS[k].label}</option>)}
            </select>
          </label>
          <label className="text-xs block" style={{ color: 'var(--text-secondary)' }}>Acciones tomadas
            <textarea value={actions} onChange={(e) => setActions(e.target.value)} maxLength={4000} rows={2} className="block w-full mt-1 px-3 py-2 rounded-lg text-sm outline-none" style={inputStyle} />
          </label>
          {status === 'cerrado' && (
            <label className="text-xs block" style={{ color: 'var(--text-secondary)' }}>Nota de cierre (obligatoria)
              <textarea value={note} onChange={(e) => setNote(e.target.value)} maxLength={2000} rows={2} className="block w-full mt-1 px-3 py-2 rounded-lg text-sm outline-none" style={inputStyle} />
            </label>
          )}
          <button disabled={busy || (status === 'cerrado' && !note.trim())} onClick={save} className="flex items-center gap-1.5 px-4 py-1.5 rounded-lg text-sm font-medium" style={{ backgroundColor: 'var(--accent)', color: '#fff' }}>
            {busy && <Loader2 size={14} className="animate-spin" />} Guardar
          </button>
        </div>
      )}
    </div>
  )
}

interface Props {
  incidents: SafetyIncident[]
  staff: boolean
  canReport: boolean
  onChange: () => void
}

/** Incidentes y casi accidentes: reportar (Usar) y revisar o cerrar (Gestionar) */
export default function IncidentsTab({ incidents, staff, canReport, onChange }: Props) {
  const [showForm, setShowForm] = useState(false)
  const [machines, setMachines] = useState<Machine[]>([])
  const [filter, setFilter] = useState<'abiertos' | 'todos'>('abiertos')

  useEffect(() => {
    fetchMachines().then(setMachines).catch(() => {})
  }, [])

  const shown = incidents.filter((i) => filter === 'todos' || i.status !== 'cerrado')

  return (
    <div className="space-y-3">
      <div className="flex items-center gap-2 flex-wrap">
        <select value={filter} onChange={(e) => setFilter(e.target.value as 'abiertos' | 'todos')} className="px-2 py-1.5 rounded-lg text-sm outline-none" style={inputStyle}>
          <option value="abiertos">Sin cerrar</option>
          <option value="todos">Todos</option>
        </select>
        {!staff && <span className="text-xs" style={{ color: 'var(--text-secondary)' }}>Aqui ves los que reportaste tu.</span>}
        {canReport && (
          <button onClick={() => setShowForm(!showForm)} className="ml-auto flex items-center gap-1.5 px-4 py-1.5 rounded-lg text-sm font-medium" style={{ backgroundColor: 'var(--accent)', color: '#fff' }}>
            <Plus size={14} /> Reportar
          </button>
        )}
      </div>
      {showForm && <ReportForm machines={machines} onDone={() => { setShowForm(false); onChange() }} />}
      {shown.length === 0 ? (
        <p className="text-sm py-6 text-center" style={{ color: 'var(--text-secondary)' }}>No hay incidentes {filter === 'abiertos' ? 'sin cerrar' : 'registrados'}.</p>
      ) : (
        shown.map((i) => <IncidentCard key={`${i.id}:${i.status}`} inc={i} staff={staff} onChange={onChange} />)
      )}
    </div>
  )
}

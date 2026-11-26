import { useEffect, useState } from 'react'
import { Loader2 } from 'lucide-react'
import { createTrip, fetchExecution, type ItemExec, type Trip } from '../../../api'
import type { PortfolioEntry } from '../../../types'
import { useAuth } from '../../../auth/AuthContext'
import { useToast } from '../../ToastContext'
import { errorMessage } from '../../../lib/errors'
import { money } from '../../../lib/finance'
import { cardStyle, inputStyle } from '../../../lib/transparency'

const field = 'block w-full mt-1 px-2 py-1.5 rounded-lg text-sm outline-none'
const label = { color: 'var(--text-secondary)' }

/** Solicitud de viaje: destino, fechas, motivo y montos estimados */
export default function TripForm({ projects, projectId, onCreated, onCancel }: { projects: PortfolioEntry[]; projectId: string | null; onCreated: (t: Trip) => void; onCancel: () => void }) {
  const { user } = useAuth()
  const { addToast } = useToast()
  const [project, setProject] = useState(projectId ?? '')
  const [items, setItems] = useState<ItemExec[]>([])
  const [item, setItem] = useState('')
  const [me, setMe] = useState(true)
  const [traveler, setTraveler] = useState('')
  const [destination, setDestination] = useState('')
  const [start, setStart] = useState('')
  const [end, setEnd] = useState('')
  const [purpose, setPurpose] = useState('')
  const [transport, setTransport] = useState('')
  const [lodging, setLodging] = useState('')
  const [perDiem, setPerDiem] = useState('')
  const [busy, setBusy] = useState(false)

  useEffect(() => {
    if (!project) return
    fetchExecution(project).then((e) => setItems(e.items)).catch(() => setItems([]))
  }, [project])

  const total = (Number(transport) || 0) + (Number(lodging) || 0) + (Number(perDiem) || 0)

  const submit = async () => {
    setBusy(true)
    try {
      const username = user?.username ?? ''
      const t = await createTrip({
        project_id: project, budget_item_id: item || null,
        traveler: me ? username : traveler, traveler_user: me ? username : null,
        destination, start_date: start, end_date: end || start, purpose,
        est_transport: Number(transport) || 0, est_lodging: Number(lodging) || 0, est_per_diem: Number(perDiem) || 0,
      })
      addToast(`Viaje V-${t.folio} solicitado: queda pendiente de aprobacion`, 'success')
      onCreated(t)
    } catch (e) {
      addToast(errorMessage(e), 'error')
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="rounded-xl p-4 space-y-3" style={cardStyle}>
      <h2 className="text-sm font-semibold" style={{ color: 'var(--text-primary)' }}>Pedir un viaje</h2>
      <div className="grid gap-3 sm:grid-cols-2">
        <label className="text-xs" style={label}>Proyecto
          <select value={project} onChange={(e) => { setProject(e.target.value); setItem(''); setItems([]) }} className={field} style={inputStyle}>
            <option value="">Elegir...</option>
            {projects.map((p) => <option key={p.id} value={p.id}>{p.title}</option>)}
          </select>
        </label>
        <label className="text-xs" style={label}>Item de presupuesto (opcional)
          <select value={item} onChange={(e) => setItem(e.target.value)} className={field} style={inputStyle} disabled={!project}>
            <option value="">Sin item</option>
            {items.map((i) => <option key={i.id} value={i.id}>{i.name} (disponible {money(i.available)})</option>)}
          </select>
        </label>
        <div className="text-xs space-y-1" style={label}>
          <label className="flex items-center gap-2"><input type="checkbox" checked={me} onChange={(e) => setMe(e.target.checked)} /> Viajo yo</label>
          {!me && <input value={traveler} maxLength={120} placeholder="Nombre de quien viaja" onChange={(e) => setTraveler(e.target.value)} className={field} style={inputStyle} />}
        </div>
        <label className="text-xs" style={label}>Destino
          <input value={destination} maxLength={160} onChange={(e) => setDestination(e.target.value)} className={field} style={inputStyle} />
        </label>
        <label className="text-xs" style={label}>Salida
          <input type="date" value={start} onChange={(e) => setStart(e.target.value)} className={field} style={inputStyle} />
        </label>
        <label className="text-xs" style={label}>Regreso
          <input type="date" value={end} min={start} onChange={(e) => setEnd(e.target.value)} className={field} style={inputStyle} />
        </label>
        <label className="text-xs sm:col-span-2" style={label}>Motivo del viaje
          <textarea rows={3} maxLength={2000} value={purpose} onChange={(e) => setPurpose(e.target.value)} className={field} style={inputStyle} />
        </label>
        <label className="text-xs" style={label}>Pasajes estimados ($)
          <input type="number" min={0} value={transport} onChange={(e) => setTransport(e.target.value)} className={field} style={inputStyle} />
        </label>
        <label className="text-xs" style={label}>Alojamiento estimado ($)
          <input type="number" min={0} value={lodging} onChange={(e) => setLodging(e.target.value)} className={field} style={inputStyle} />
        </label>
        <label className="text-xs" style={label}>Viatico estimado ($)
          <input type="number" min={0} value={perDiem} onChange={(e) => setPerDiem(e.target.value)} className={field} style={inputStyle} />
        </label>
        <p className="text-sm self-end" style={{ color: 'var(--text-primary)' }}>Total estimado: <strong>{money(total)}</strong></p>
      </div>
      <p className="text-xs" style={label}>La aprobacion la da otra persona que gestione Finanzas: quien pide o viaja no puede aprobar su propio viaje.</p>
      <div className="flex gap-2">
        <button onClick={submit} disabled={busy || !project || !destination.trim() || !start || !purpose.trim() || total <= 0 || (!me && !traveler.trim())}
          className="inline-flex items-center gap-2 px-3 py-2 rounded-lg text-sm font-medium disabled:opacity-50" style={{ backgroundColor: 'var(--accent)', color: '#fff' }}>
          {busy && <Loader2 size={14} className="animate-spin" />} Pedir viaje
        </button>
        <button onClick={onCancel} className="px-3 py-2 rounded-lg text-sm" style={{ border: '1px solid var(--border)', color: 'var(--text-primary)' }}>Cancelar</button>
      </div>
    </div>
  )
}

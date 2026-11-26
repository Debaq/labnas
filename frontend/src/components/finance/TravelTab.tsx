import { useCallback, useEffect, useState } from 'react'
import { Loader2, Plus } from 'lucide-react'
import { fetchTrip, fetchTrips, type Trip } from '../../api'
import { useEvent } from '../../events/useEvents'
import { useToast } from '../ToastContext'
import { errorMessage } from '../../lib/errors'
import { money, type FinanceTabProps } from '../../lib/finance'
import { TRIP_STATUS } from '../../lib/financeTravel'
import { cardStyle, fmtDay } from '../../lib/transparency'
import TripDetail from './travel/TripDetail'
import TripForm from './travel/TripForm'

/** Viajes y viaticos: solicitud, aprobacion, comprobantes e informe de comision (docs/FINANZAS.md) */
export default function TravelTab({ projectId, projects, staff, canUse }: FinanceTabProps) {
  const { addToast } = useToast()
  const [trips, setTrips] = useState<Trip[] | null>(null)
  const [selected, setSelected] = useState<Trip | null>(null)
  const [creating, setCreating] = useState(false)

  const load = useCallback(() => {
    fetchTrips(projectId).then(setTrips).catch((e) => { setTrips([]); addToast(errorMessage(e), 'error') })
  }, [projectId, addToast])
  useEffect(load, [load])
  const refreshSelected = useCallback(() => { setSelected((s) => { if (s) fetchTrip(s.id).then(setSelected).catch(() => undefined); return s }) }, [])
  useEvent('finance', () => { load(); refreshSelected() })

  const open = (t: Trip) => { setCreating(false); fetchTrip(t.id).then(setSelected).catch((e) => addToast(errorMessage(e), 'error')) }
  const changed = (t: Trip) => { setSelected(t); load() }

  return (
    <div className="space-y-4">
      <div className="flex items-center gap-2">
        <p className="flex-1 text-sm" style={{ color: 'var(--text-secondary)' }}>Viajes del proyecto: se piden, los aprueba otra persona, se rinden con comprobantes y un informe de comision.</p>
        {canUse && (
          <button onClick={() => { setSelected(null); setCreating(true) }} className="inline-flex items-center gap-1 px-3 py-2 rounded-lg text-sm font-medium" style={{ backgroundColor: 'var(--accent)', color: '#fff' }}>
            <Plus size={14} /> Pedir viaje
          </button>
        )}
      </div>
      {creating && <TripForm projects={projects} projectId={projectId} onCancel={() => setCreating(false)} onCreated={(t) => { setCreating(false); load(); open(t) }} />}
      {selected && <TripDetail key={selected.id + selected.status + (selected.receipts?.length ?? 0)} trip={selected} staff={staff} canUse={canUse} onChange={changed} />}
      <div className="rounded-xl overflow-hidden" style={cardStyle}>
        {trips == null && <div className="p-4"><Loader2 className="animate-spin" size={18} style={{ color: 'var(--accent)' }} /></div>}
        {trips?.length === 0 && <p className="p-4 text-sm" style={{ color: 'var(--text-secondary)' }}>No hay viajes{projectId ? ' en este proyecto' : ''}.</p>}
        <ul className="divide-y" style={{ borderColor: 'var(--card-border)' }}>
          {trips?.map((t) => {
            const st = TRIP_STATUS[t.status]
            return (
              <li key={t.id}>
                <button onClick={() => open(t)} className="w-full text-left px-4 py-3 flex items-center gap-3">
                  <span className="font-mono text-xs" style={{ color: 'var(--text-secondary)' }}>V-{t.folio}</span>
                  <span className="flex-1 min-w-0">
                    <span className="block text-sm truncate" style={{ color: 'var(--text-primary)' }}>{t.destination} · {t.traveler}</span>
                    <span className="block text-xs truncate" style={{ color: 'var(--text-secondary)' }}>{t.project_title} · {fmtDay(t.start_date)} al {fmtDay(t.end_date)}</span>
                  </span>
                  <span className="text-sm" style={{ color: 'var(--text-primary)' }}>{money(t.status === 'cerrado' ? t.spent : t.est_total)}</span>
                  <span className="text-[11px] px-2 py-0.5 rounded-full whitespace-nowrap" style={{ color: st.color, border: '1px solid currentColor' }}>{st.label}</span>
                </button>
              </li>
            )
          })}
        </ul>
      </div>
    </div>
  )
}

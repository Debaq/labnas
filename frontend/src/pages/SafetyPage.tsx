import { useCallback, useEffect, useState } from 'react'
import { Loader2, ShieldAlert } from 'lucide-react'
import {
  fetchIncidents,
  fetchMyTrainings,
  fetchSheets,
  fetchTrainings,
  type MyTraining,
  type SafetyIncident,
  type SafetySheet,
  type SafetyTraining,
} from '../api'
import { useAuth } from '../auth/AuthContext'
import { useEvent } from '../events/useEvents'
import { useToast } from '../components/ToastContext'
import { errorMessage } from '../lib/errors'
import IncidentsTab from '../components/safety/IncidentsTab'
import TrainingsTab from '../components/safety/TrainingsTab'
import SheetsTab from '../components/safety/SheetsTab'
import PresenceTab from '../components/safety/PresenceTab'

type Tab = 'incidentes' | 'capacitaciones' | 'fichas' | 'presencia'

export default function SafetyPage() {
  const { canModule } = useAuth()
  const staff = canModule('seguridad', 'gestionar')
  const canReport = canModule('seguridad', 'usar')
  const { addToast } = useToast()
  // El QR de la puerta abre /safety?entrada=1 directo en el registro de entrada
  const [tab, setTab] = useState<Tab>(() => (new URLSearchParams(window.location.search).has('entrada') ? 'presencia' : 'incidentes'))
  const [loading, setLoading] = useState(true)
  const [incidents, setIncidents] = useState<SafetyIncident[]>([])
  const [trainings, setTrainings] = useState<SafetyTraining[]>([])
  const [mine, setMine] = useState<MyTraining[]>([])
  const [sheets, setSheets] = useState<SafetySheet[]>([])

  const load = useCallback(async () => {
    try {
      const [i, t, m, s] = await Promise.all([fetchIncidents(), fetchTrainings(), fetchMyTrainings(), fetchSheets()])
      setIncidents(i)
      setTrainings(t)
      setMine(m)
      setSheets(s)
    } catch (e) {
      addToast(errorMessage(e), 'error')
    } finally {
      setLoading(false)
    }
  }, [addToast])

  useEffect(() => { load() }, [load])
  useEvent('safety', load, load)

  const open = incidents.filter((i) => i.status !== 'cerrado').length
  const missing = mine.filter((m) => m.training.mandatory && (m.status === 'pendiente' || m.status === 'vencida')).length
  const tabs: { id: Tab; label: string; badge: number }[] = [
    { id: 'incidentes', label: 'Incidentes', badge: open },
    { id: 'capacitaciones', label: 'Capacitaciones', badge: missing },
    { id: 'fichas', label: 'Fichas', badge: 0 },
    { id: 'presencia', label: 'Quien esta', badge: 0 },
  ]

  return (
    <div className="p-4 sm:p-6 max-w-4xl mx-auto space-y-4">
      <h1 className="text-xl font-semibold flex items-center gap-2" style={{ color: 'var(--text-primary)' }}>
        <ShieldAlert size={20} /> Seguridad
      </h1>
      <div className="flex gap-1" style={{ borderBottom: '1px solid var(--border)' }}>
        {tabs.map((t) => (
          <button key={t.id} onClick={() => setTab(t.id)} className="px-4 py-2 text-sm font-medium flex items-center gap-1.5"
            style={{ color: tab === t.id ? 'var(--accent)' : 'var(--text-secondary)', borderBottom: tab === t.id ? '2px solid var(--accent)' : '2px solid transparent' }}>
            {t.label}
            {t.badge > 0 && <span className="text-[11px] px-1.5 rounded-full" style={{ backgroundColor: 'var(--danger)', color: '#fff' }}>{t.badge}</span>}
          </button>
        ))}
      </div>
      {loading ? (
        <div className="flex justify-center py-10"><Loader2 className="animate-spin" /></div>
      ) : tab === 'incidentes' ? (
        <IncidentsTab incidents={incidents} staff={staff} canReport={canReport} onChange={load} />
      ) : tab === 'capacitaciones' ? (
        <TrainingsTab trainings={trainings} mine={mine} staff={staff} onChange={load} />
      ) : tab === 'presencia' ? (
        <PresenceTab staff={staff} />
      ) : (
        <SheetsTab sheets={sheets} staff={staff} onChange={load} />
      )}
    </div>
  )
}

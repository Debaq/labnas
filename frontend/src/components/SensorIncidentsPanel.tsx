import { useCallback, useEffect, useState } from 'react'
import { BellRing, Check } from 'lucide-react'
import { ackSensorIncident, fetchSensorIncidents } from '../api'
import type { SensorIncident } from '../types'
import { useEvent } from '../events/useEvents'
import { READING_LABELS, READING_UNITS } from '../lib/sensors'
import { errorMessage } from '../lib/errors'

function when(s: string) {
  return new Date(s).toLocaleString('es-CL', { day: '2-digit', month: '2-digit', hour: '2-digit', minute: '2-digit' })
}

function line(i: SensorIncident) {
  const unit = READING_UNITS[i.key] ?? ''
  return `${READING_LABELS[i.key] ?? i.key} ${i.worst_value}${unit} (${i.condition === 'below' ? 'bajo' : 'sobre'} ${i.threshold}${unit})`
}

/** Alarmas criticas: las abiertas arriba con «Lo vi»; el historial plegado */
export default function SensorIncidentsPanel() {
  const [list, setList] = useState<SensorIncident[]>([])
  const [error, setError] = useState<string | null>(null)
  const load = useCallback(() => { fetchSensorIncidents().then(setList).catch(() => {}) }, [])
  useEffect(() => { load() }, [load])
  useEvent('sensors.incidents', load, load)

  const open = list.filter(i => !i.resolved_at)
  const past = list.filter(i => i.resolved_at)
  if (list.length === 0) return null

  async function ack(id: string) {
    try { await ackSensorIncident(id); setError(null); load() } catch (e) { setError(errorMessage(e)) }
  }

  return (
    <div className="space-y-2">
      {error && <p className="text-sm" style={{ color: 'var(--danger)' }}>{error}</p>}
      {open.map(i => (
        <div key={i.id} className="p-4 rounded-xl flex items-center justify-between gap-3 flex-wrap" style={{ backgroundColor: 'color-mix(in srgb, var(--danger) 12%, transparent)', border: '1px solid var(--danger)' }}>
          <div className="min-w-0">
            <div className="text-sm font-semibold flex items-center gap-2" style={{ color: 'var(--text-primary)' }}>
              <BellRing size={16} style={{ color: 'var(--danger)' }} /> Alarma critica: {i.device_name}
            </div>
            <div className="text-xs" style={{ color: 'var(--text-secondary)' }}>
              {line(i)} · desde {when(i.started_at)} · avisado a {i.notified.join(', ') || '—'}
              {i.acked_by && <> · <span style={{ color: 'var(--success)' }}>lo atiende {i.acked_by}</span></>}
            </div>
          </div>
          {!i.acked_by && (
            <button onClick={() => ack(i.id)} className="flex items-center gap-1 px-3 py-1.5 rounded-lg text-sm" style={{ backgroundColor: 'var(--danger)', color: '#fff' }}>
              <Check size={14} /> Lo vi, lo atiendo
            </button>
          )}
        </div>
      ))}
      {past.length > 0 && (
        <details>
          <summary className="text-sm cursor-pointer" style={{ color: 'var(--text-secondary)' }}>Historial de alarmas criticas ({past.length})</summary>
          <ul className="mt-2 space-y-1">
            {past.map(i => (
              <li key={i.id} className="text-xs px-3 py-2 rounded-lg" style={{ backgroundColor: 'var(--bg-secondary)', color: 'var(--text-primary)' }}>
                <b>{i.device_name}</b> · {line(i)} · {when(i.started_at)} → {when(i.resolved_at!)}
                <span style={{ color: i.acked_by ? 'var(--text-secondary)' : 'var(--warning, #d97706)' }}>
                  {i.acked_by ? ` · la atendio ${i.acked_by}` : ' · nadie la confirmo'}
                </span>
              </li>
            ))}
          </ul>
        </details>
      )}
    </div>
  )
}

import { useCallback, useEffect, useRef, useState } from 'react'
import { Link } from 'react-router-dom'
import { Thermometer } from 'lucide-react'
import { fetchSensorLatest } from '../../api'
import type { SensorLatest } from '../../types'
import { useEvent } from '../../events/useEvents'
import { READING_LABELS, READING_UNITS } from '../../lib/sensors'

/** Dashboard: ultima lectura de cada sensor activo (maximo 6) */
export default function SensorsWidget() {
  const [latest, setLatest] = useState<SensorLatest[] | null>(null)

  const load = useCallback(() => {
    fetchSensorLatest().then(setLatest).catch(() => setLatest([]))
  }, [])
  // Los sensores pueden mandar datos muy seguido: como mucho una recarga cada 5 s
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null)
  const schedule = useCallback(() => {
    if (timer.current) return
    timer.current = setTimeout(() => { timer.current = null; load() }, 5000)
  }, [load])
  useEffect(() => () => { if (timer.current) clearTimeout(timer.current) }, [])

  useEffect(() => { load() }, [load])
  useEvent('sensors.updated', schedule, load)

  const list = (latest ?? []).filter((l) => l.device && l.device.status === 'accepted' && Object.keys(l.readings).length > 0).slice(0, 6)
  if (list.length === 0) return null

  return (
    <div className="rounded-xl p-6" style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}>
      <div className="flex items-center gap-3 mb-4">
        <Thermometer size={22} style={{ color: 'var(--accent)' }} />
        <h2 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>Sensores</h2>
        <Link to="/sensors" className="ml-auto text-xs hover:underline" style={{ color: 'var(--accent)' }}>Ver todos</Link>
      </div>
      <div className="grid gap-3 grid-cols-1 sm:grid-cols-2">
        {list.map((l) => (
          <div key={l.device!.id} className="rounded-lg p-3" style={{ backgroundColor: 'var(--bg-tertiary)' }}>
            <div className="flex items-center gap-2 mb-1">
              <span className="w-2 h-2 rounded-full" style={{ backgroundColor: l.online ? 'var(--success)' : 'var(--text-secondary)' }} title={l.online ? 'En linea' : 'Sin datos recientes'} />
              <span className="text-sm font-medium truncate" style={{ color: 'var(--text-primary)' }}>{l.device!.name}</span>
            </div>
            <div className="flex flex-wrap gap-x-3 gap-y-0.5 text-xs" style={{ color: 'var(--text-secondary)' }}>
              {Object.entries(l.readings).slice(0, 4).map(([k, v]) => (
                <span key={k}>
                  {READING_LABELS[k] ?? k}{' '}
                  <b style={{ color: 'var(--text-primary)' }}>{Number.isInteger(v) ? v : v.toFixed(1)}{READING_UNITS[k] ? ` ${READING_UNITS[k]}` : ''}</b>
                </span>
              ))}
            </div>
          </div>
        ))}
      </div>
    </div>
  )
}

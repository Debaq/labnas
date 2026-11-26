import { useCallback, useEffect, useState } from 'react'
import { Link } from 'react-router-dom'
import { Wrench, AlertTriangle } from 'lucide-react'
import { fetchMachines, fetchBookings, type Machine, type MachineBooking } from '../../api'
import { useEvent } from '../../events/useEvents'

/** `YYYY-MM-DDTHH:MM` local */
function localNow(): string {
  const d = new Date()
  const p = (n: number) => String(n).padStart(2, '0')
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}T${p(d.getHours())}:${p(d.getMinutes())}`
}

function when(start: string): string {
  const [date, time] = start.split('T')
  const today = localNow().slice(0, 10)
  if (date === today) return `hoy ${time}`
  const [, m, d] = date.split('-')
  return `${d}/${m} ${time}`
}

/** Dashboard: maquinas en uso o fuera de servicio y mis proximas reservas */
export default function MachinesWidget() {
  const [machines, setMachines] = useState<Machine[] | null>(null)
  const [mine, setMine] = useState<MachineBooking[]>([])

  const load = useCallback(() => {
    fetchMachines().then(setMachines).catch(() => setMachines([]))
    const now = localNow()
    fetchBookings({ mine: true, from: now })
      // Solo las que aun no empiezan (la que esta en curso sale como "en uso")
      .then((b) => setMine(b.filter((x) => x.kind === 'reserva' && x.start > now && (x.status === 'confirmada' || x.status === 'pendiente')).slice(0, 4)))
      .catch(() => setMine([]))
  }, [])

  useEffect(() => { load() }, [load])
  useEvent('machines', load, load)

  if (!machines || machines.length === 0) return null
  const active = machines.filter((m) => m.status !== 'retirada')
  const inUse = active.filter((m) => m.current)
  const broken = active.filter((m) => m.status === 'fuera_servicio')
  const free = active.length - inUse.length - broken.length

  return (
    <div className="rounded-xl p-6" style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}>
      <div className="flex items-center gap-3 mb-4">
        <Wrench size={22} style={{ color: 'var(--accent)' }} />
        <h2 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>Maquinas</h2>
        <Link to="/machines" className="ml-auto text-xs hover:underline" style={{ color: 'var(--accent)' }}>Ver todas</Link>
      </div>
      <div className="grid grid-cols-3 gap-3 mb-4 text-center">
        <div><div className="text-xl font-bold" style={{ color: 'var(--success)' }}>{free}</div><div className="text-xs" style={{ color: 'var(--text-secondary)' }}>libres</div></div>
        <div><div className="text-xl font-bold" style={{ color: 'var(--accent)' }}>{inUse.length}</div><div className="text-xs" style={{ color: 'var(--text-secondary)' }}>en uso</div></div>
        <div><div className="text-xl font-bold" style={{ color: broken.length ? 'var(--danger)' : 'var(--text-secondary)' }}>{broken.length}</div><div className="text-xs" style={{ color: 'var(--text-secondary)' }}>fuera de servicio</div></div>
      </div>
      <div className="space-y-2 text-sm">
        {broken.map((m) => (
          <Link key={m.id} to={`/machines/${m.id}`} className="flex items-center gap-2 hover:underline" style={{ color: 'var(--danger)' }}>
            <AlertTriangle size={14} />
            <span className="truncate">{m.name}{m.open_faults[0] ? `: ${m.open_faults[0].description}` : ''}</span>
          </Link>
        ))}
        {inUse.map((m) => (
          <Link key={m.id} to={`/machines/${m.id}`} className="flex items-center justify-between gap-2 hover:underline" style={{ color: 'var(--text-primary)' }}>
            <span className="truncate">{m.name}</span>
            <span className="text-xs whitespace-nowrap" style={{ color: 'var(--text-secondary)' }}>{m.current?.username} hasta {m.current?.end.slice(11)}</span>
          </Link>
        ))}
        {mine.length > 0 && (
          <div className="pt-2" style={{ borderTop: '1px solid var(--border)' }}>
            <div className="text-xs mb-1" style={{ color: 'var(--text-secondary)' }}>Mis proximas reservas</div>
            {mine.map((b) => (
              <Link key={b.id} to={`/machines/${b.machine_id}`} className="flex items-center justify-between gap-2 hover:underline" style={{ color: 'var(--text-primary)' }}>
                <span className="truncate">{b.machine_name}</span>
                <span className="text-xs whitespace-nowrap" style={{ color: b.status === 'pendiente' ? 'var(--warning)' : 'var(--text-secondary)' }}>
                  {when(b.start)}{b.status === 'pendiente' ? ' · por aprobar' : ''}
                </span>
              </Link>
            ))}
          </div>
        )}
      </div>
    </div>
  )
}

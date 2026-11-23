import { hhmm, type Machine } from '../../api'

/** Estado de una maquina: fuera de servicio, retirada, en uso o disponible */
function machineState(m: Machine): { label: string; color: string } {
  if (m.status === 'fuera_servicio') return { label: 'Fuera de servicio', color: 'var(--danger)' }
  if (m.status === 'retirada') return { label: 'Retirada', color: 'var(--text-secondary)' }
  if (m.current) {
    const who = m.current.kind === 'bloqueo' ? 'Bloqueada' : `En uso · ${m.current.username}`
    return { label: `${who} hasta ${hhmm(m.current.end)}`, color: 'var(--warning, #d97706)' }
  }
  return { label: 'Disponible', color: 'var(--success)' }
}

export default function MachineStatusChip({ machine }: { machine: Machine }) {
  const st = machineState(machine)
  return (
    <span
      className="inline-flex items-center gap-1 text-[11px] px-2 py-0.5 rounded-full font-medium whitespace-nowrap"
      style={{ color: st.color, backgroundColor: `color-mix(in srgb, ${st.color} 14%, transparent)` }}
    >
      <span className="w-1.5 h-1.5 rounded-full" style={{ backgroundColor: st.color }} />
      {st.label}
    </span>
  )
}

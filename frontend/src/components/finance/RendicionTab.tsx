import { useState } from 'react'
import type { FinanceTabProps } from '../../lib/finance'
import RendicionSection from './rendicion/RendicionSection'
import ExpedienteSection from './rendicion/ExpedienteSection'
import ManualSection from './rendicion/ManualSection'
import AnomaliasSection from './rendicion/AnomaliasSection'
import HorasSection from './rendicion/HorasSection'

type Sub = 'rendicion' | 'expediente' | 'manual' | 'anomalias' | 'horas'

const SUBS: { id: Sub; label: string }[] = [
  { id: 'rendicion', label: 'Rendicion' },
  { id: 'expediente', label: 'Expediente' },
  { id: 'manual', label: 'Manual' },
  { id: 'anomalias', label: 'Anomalias' },
  { id: 'horas', label: 'Horas' },
]

/** Rendicion y auditoria: rendicion, expediente, manual de procedimientos, anomalias y horas */
export default function RendicionTab(props: FinanceTabProps) {
  const [sub, setSub] = useState<Sub>('rendicion')
  return (
    <div className="space-y-4">
      <div className="flex gap-2 flex-wrap">
        {SUBS.map((s) => (
          <button key={s.id} onClick={() => setSub(s.id)} className="px-3 py-1.5 rounded-lg text-sm"
            style={{
              backgroundColor: sub === s.id ? 'var(--accent)' : 'var(--card-bg)',
              color: sub === s.id ? 'var(--accent-text, #fff)' : 'var(--text-secondary)',
              border: '1px solid var(--card-border)',
            }}>
            {s.label}
          </button>
        ))}
      </div>
      {sub === 'rendicion' && <RendicionSection {...props} />}
      {sub === 'expediente' && <ExpedienteSection {...props} />}
      {sub === 'manual' && <ManualSection {...props} />}
      {sub === 'anomalias' && <AnomaliasSection {...props} />}
      {sub === 'horas' && <HorasSection {...props} />}
    </div>
  )
}

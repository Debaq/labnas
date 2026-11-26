import { useState } from 'react'
import type { FinanceTabProps } from '../../lib/finance'
import ExecutionPanel from './budget/ExecutionPanel'
import AssetsPanel from './budget/AssetsPanel'
import PettyPanel from './budget/PettyPanel'
import IncomesPanel from './budget/IncomesPanel'

type Section = 'ejecucion' | 'bienes' | 'caja' | 'ingresos'

const SECTIONS: { id: Section; label: string }[] = [
  { id: 'ejecucion', label: 'Ejecución' },
  { id: 'bienes', label: 'Bienes del proyecto' },
  { id: 'caja', label: 'Caja chica' },
  { id: 'ingresos', label: 'Ingresos' },
]

/** Presupuesto por ítem, ejecución en vivo, bienes, caja chica e ingresos (docs/FINANZAS.md) */
export default function BudgetTab(props: FinanceTabProps) {
  const [section, setSection] = useState<Section>('ejecucion')
  return (
    <div className="space-y-4">
      <div className="flex gap-2 flex-wrap">
        {SECTIONS.map((s) => (
          <button key={s.id} onClick={() => setSection(s.id)} className="px-3 py-1.5 rounded-lg text-xs font-medium"
            style={{
              backgroundColor: section === s.id ? 'var(--accent)' : 'var(--input-bg)',
              color: section === s.id ? '#fff' : 'var(--text-secondary)',
              border: '1px solid var(--input-border)',
            }}>
            {s.label}
          </button>
        ))}
      </div>
      {section === 'ejecucion' ? <ExecutionPanel key={props.projectId ?? 'todos'} {...props} />
        : section === 'bienes' ? <AssetsPanel key={props.projectId ?? 'todos'} {...props} />
        : section === 'caja' ? <PettyPanel key={props.projectId ?? 'todos'} {...props} />
        : <IncomesPanel key={props.projectId ?? 'todos'} {...props} />}
    </div>
  )
}

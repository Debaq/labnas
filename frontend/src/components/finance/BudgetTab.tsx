import type { FinanceTabProps } from '../../lib/finance'

/** Presupuesto (por implementar) */
export default function BudgetTab({ projectId }: FinanceTabProps) {
  return <p className="text-sm" style={{ color: 'var(--text-secondary)' }}>Presupuesto: en construccion{projectId ? ' (proyecto elegido)' : ''}.</p>
}

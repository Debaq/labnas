import type { FinanceTabProps } from '../../lib/finance'

/** Compras (por implementar) */
export default function PurchasesTab({ projectId }: FinanceTabProps) {
  return <p className="text-sm" style={{ color: 'var(--text-secondary)' }}>Compras: en construccion{projectId ? ' (proyecto elegido)' : ''}.</p>
}

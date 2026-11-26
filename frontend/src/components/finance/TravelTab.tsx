import type { FinanceTabProps } from '../../lib/finance'

/** Viajes (por implementar) */
export default function TravelTab({ projectId }: FinanceTabProps) {
  return <p className="text-sm" style={{ color: 'var(--text-secondary)' }}>Viajes: en construccion{projectId ? ' (proyecto elegido)' : ''}.</p>
}

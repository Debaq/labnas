import type { FinanceTabProps } from '../../lib/finance'

/** Rendicion (por implementar) */
export default function RendicionTab({ projectId }: FinanceTabProps) {
  return <p className="text-sm" style={{ color: 'var(--text-secondary)' }}>Rendicion: en construccion{projectId ? ' (proyecto elegido)' : ''}.</p>
}

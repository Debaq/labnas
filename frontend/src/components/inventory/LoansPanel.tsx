import { useCallback, useEffect, useState } from 'react'
import { HandHelping, Undo2 } from 'lucide-react'
import { fetchLoans, returnLoan } from '../../api'
import type { InventoryLoan } from '../../types'
import { useAuth } from '../../auth/AuthContext'
import { useEvent } from '../../events/useEvents'
import { dueText } from '../../lib/inventory'

/** Prestamos abiertos: los propios (o todos, para quien gestiona Inventario) */
export default function LoansPanel({ onChange }: { onChange: () => void }) {
  const { user, canModule } = useAuth()
  const staff = canModule('inventory', 'gestionar')
  const [loans, setLoans] = useState<InventoryLoan[]>([])
  const load = useCallback(() => { fetchLoans({ open: true }).then(setLoans).catch(() => {}) }, [])
  useEffect(() => { load() }, [load])
  useEvent('inventory.loans', load, load)
  if (loans.length === 0) return null
  const late = loans.filter(l => l.days_left < 0).length

  return (
    <details className="rounded-xl px-4 py-3" open={late > 0} style={{ backgroundColor: 'var(--card-bg)', border: `1px solid ${late ? 'var(--danger)' : 'var(--card-border)'}` }}>
      <summary className="text-sm font-medium cursor-pointer flex items-center gap-2" style={{ color: 'var(--text-primary)' }}>
        <HandHelping size={15} /> {staff ? 'Prestamos abiertos' : 'Lo que tengo prestado'} ({loans.length}){late > 0 && <span style={{ color: 'var(--danger)' }}> · {late} atrasado{late > 1 ? 's' : ''}</span>}
      </summary>
      <div className="mt-2 space-y-1">
        {loans.map(l => (
          <div key={l.id} className="flex items-center justify-between gap-2 text-xs px-2 py-1 rounded" style={{ backgroundColor: 'var(--bg-secondary)' }}>
            <span style={{ color: 'var(--text-primary)' }}>
              <b>{l.item_name}</b> ({l.quantity}){staff && <> · {l.borrower}</>} · <span style={{ color: l.days_left < 0 ? 'var(--danger)' : 'var(--text-secondary)' }}>{dueText(l)}</span>
            </span>
            {(staff || l.borrower === user?.username) && (
              <button onClick={async () => { await returnLoan(l.id).catch(() => {}); load(); onChange() }} className="flex items-center gap-1" style={{ color: 'var(--accent)' }}>
                <Undo2 size={12} /> {l.borrower === user?.username ? 'Devolver' : 'Recibir'}
              </button>
            )}
          </div>
        ))}
      </div>
    </details>
  )
}

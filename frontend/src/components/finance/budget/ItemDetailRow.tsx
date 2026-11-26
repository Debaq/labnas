import { useEffect, useState } from 'react'
import { fetchItemDetail, type ItemDetail } from '../../../api'
import { money } from '../../../lib/finance'
import { fmtDay } from '../../../lib/financeBudget'

const SOURCE: Record<string, string> = { compra: 'Compra', viaje: 'Viaje', caja: 'Caja chica', honorario: 'Honorarios' }

/** Historial de montos y compromisos de un ítem */
export default function ItemDetailRow({ itemId, currency, version }: { itemId: string; currency: string; version: number }) {
  const [d, setD] = useState<ItemDetail | null>(null)
  useEffect(() => {
    fetchItemDetail(itemId).then(setD).catch(() => setD(null))
  }, [itemId, version])
  if (!d) return <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>Cargando…</p>
  return (
    <div className="grid gap-4 md:grid-cols-2 text-xs" style={{ color: 'var(--text-primary)' }}>
      <div>
        <h4 className="font-semibold mb-1">Compromisos y gastos</h4>
        {d.commitments.length === 0 ? <p style={{ color: 'var(--text-secondary)' }}>Nada comprometido aún.</p> : (
          <ul className="space-y-1">
            {d.commitments.map((c) => (
              <li key={c.source + c.source_id}>
                {SOURCE[c.source] ?? c.source}: {c.label || c.source_id} — comprometido {money(c.committed, currency)}, gastado {money(c.spent, currency)}
              </li>
            ))}
          </ul>
        )}
      </div>
      <div>
        <h4 className="font-semibold mb-1">Cambios del monto aprobado</h4>
        {d.changes.length === 0 ? <p style={{ color: 'var(--text-secondary)' }}>Sin cambios desde que se creó.</p> : (
          <ul className="space-y-1">
            {d.changes.map((c, i) => (
              <li key={i}>
                {fmtDay(c.changed_at)} · {c.changed_by}: {money(c.old_amount, currency)} → {money(c.new_amount, currency)} — «{c.reason}»{c.ledger_seq != null && ` (bitácora #${c.ledger_seq})`}
              </li>
            ))}
          </ul>
        )}
      </div>
    </div>
  )
}

import { useEffect, useState } from 'react'
import { api } from '../../../api/client'
import { finJsonOrThrow } from '../../../api/finance'
import { createPurchase, type PurchaseDetail } from '../../../api/financePurchases'
import type { PortfolioEntry } from '../../../types'
import { errorMessage } from '../../../lib/errors'
import { cardStyle, inputStyle } from '../../../lib/financePurchases'
import { money } from '../../../lib/finance'
import { useToast } from '../../ToastContext'

interface BudgetItem { id: string; name: string; approved: number; available: number }

/** Solicitud de compra: proyecto, item de presupuesto (opcional), titulo, justificacion y monto */
export default function NewPurchaseForm({ projects, projectId, onCreated, onCancel }: {
  projects: PortfolioEntry[]
  projectId: string | null
  onCreated: (d: PurchaseDetail) => void
  onCancel: () => void
}) {
  const { addToast } = useToast()
  const [project, setProject] = useState(projectId ?? '')
  const [items, setItems] = useState<BudgetItem[]>([])
  const [item, setItem] = useState('')
  const [title, setTitle] = useState('')
  const [why, setWhy] = useState('')
  const [amount, setAmount] = useState('')
  const [busy, setBusy] = useState(false)

  useEffect(() => {
    if (!project) return
    let live = true
    api(`/api/finance/purchases/items?project=${encodeURIComponent(project)}`)
      .then((r) => finJsonOrThrow<BudgetItem[]>(r, 'No se pudieron cargar los items'))
      .then((x) => { if (live) { setItems(x); setItem('') } })
      .catch(() => { if (live) setItems([]) })
    return () => { live = false }
  }, [project])

  const chosen = items.find((i) => i.id === item)
  const n = Number(amount.replace(/\./g, '').replace(',', '.'))
  const over = chosen && Number.isFinite(n) && n > chosen.available

  const submit = async () => {
    setBusy(true)
    try {
      const d = await createPurchase({ project_id: project, budget_item_id: item || null, title, justification: why, estimated_amount: n })
      addToast(`Solicitud #${d.purchase.folio} creada`, 'success')
      onCreated(d)
    } catch (e) {
      addToast(errorMessage(e), 'error')
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="rounded-xl p-4 space-y-3" style={cardStyle}>
      <h2 className="text-sm font-semibold" style={{ color: 'var(--text-primary)' }}>Nueva solicitud de compra</h2>
      <div className="grid gap-3 sm:grid-cols-2">
        <label className="text-xs" style={{ color: 'var(--text-secondary)' }}>Proyecto
          <select value={project} onChange={(e) => setProject(e.target.value)} className="block w-full mt-1 px-2 py-1.5 rounded-lg text-sm outline-none" style={inputStyle}>
            <option value="">Elige un proyecto</option>
            {projects.map((p) => <option key={p.id} value={p.id}>{p.title}</option>)}
          </select>
        </label>
        <label className="text-xs" style={{ color: 'var(--text-secondary)' }}>Item de presupuesto (opcional)
          <select value={item} onChange={(e) => setItem(e.target.value)} className="block w-full mt-1 px-2 py-1.5 rounded-lg text-sm outline-none" style={inputStyle}>
            <option value="">Sin item</option>
            {items.map((i) => <option key={i.id} value={i.id}>{i.name} (disponible {money(i.available)})</option>)}
          </select>
        </label>
        <label className="text-xs sm:col-span-2" style={{ color: 'var(--text-secondary)' }}>Que se quiere comprar
          <input value={title} onChange={(e) => setTitle(e.target.value)} maxLength={200} className="block w-full mt-1 px-2 py-1.5 rounded-lg text-sm outline-none" style={inputStyle} />
        </label>
        <label className="text-xs sm:col-span-2" style={{ color: 'var(--text-secondary)' }}>Justificacion (obligatoria)
          <textarea value={why} onChange={(e) => setWhy(e.target.value)} maxLength={2000} rows={3} placeholder="Para que se necesita y por que ahora" className="block w-full mt-1 px-2 py-1.5 rounded-lg text-sm outline-none" style={inputStyle} />
        </label>
        <label className="text-xs" style={{ color: 'var(--text-secondary)' }}>Monto estimado ($)
          <input inputMode="numeric" value={amount} onChange={(e) => setAmount(e.target.value)} className="block w-full mt-1 px-2 py-1.5 rounded-lg text-sm outline-none" style={inputStyle} />
        </label>
      </div>
      {over && <p className="text-xs" style={{ color: 'var(--danger)' }}>Ojo: el monto supera lo disponible del item ({money(chosen.available)}). Se puede pedir igual; quien aprueba lo vera.</p>}
      <div className="flex gap-2 justify-end">
        <button onClick={onCancel} className="px-3 py-1.5 rounded-lg text-sm" style={inputStyle}>Cancelar</button>
        <button onClick={submit} disabled={busy || !project || !title.trim() || why.trim().length < 10 || !(n > 0)} className="px-3 py-1.5 rounded-lg text-sm font-medium disabled:opacity-50" style={{ backgroundColor: 'var(--accent)', color: '#fff' }}>
          {busy ? 'Enviando...' : 'Pedir compra'}
        </button>
      </div>
    </div>
  )
}

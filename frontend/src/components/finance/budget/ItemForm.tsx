import { useState } from 'react'
import { BUDGET_CATEGORIES, createBudgetItem, updateBudgetItem, type ItemExec } from '../../../api'
import { useToast } from '../../ToastContext'
import { errorMessage } from '../../../lib/errors'
import { CATEGORY_LABEL, cardStyle, inputStyle } from '../../../lib/financeBudget'

/** Crea o edita un ítem del presupuesto; cambiar el monto aprobado exige motivo */
export default function ItemForm({ projectId, item, onDone, onCancel }: {
  projectId: string
  item: ItemExec | null
  onDone: () => void
  onCancel: () => void
}) {
  const { addToast } = useToast()
  const [category, setCategory] = useState(item?.category ?? 'equipos')
  const [name, setName] = useState(item?.name ?? '')
  const [approved, setApproved] = useState(String(item?.approved ?? ''))
  const [notes, setNotes] = useState(item?.notes ?? '')
  const [reason, setReason] = useState('')
  const [busy, setBusy] = useState(false)
  const amountChanged = item != null && Number(approved) !== item.approved

  const submit = async () => {
    setBusy(true)
    try {
      const body = { category, name, approved: Number(approved), notes, reason }
      if (item) await updateBudgetItem(item.id, body)
      else await createBudgetItem({ ...body, project_id: projectId })
      addToast(item ? 'Ítem actualizado' : 'Ítem creado', 'success')
      onDone()
    } catch (e) {
      addToast(errorMessage(e), 'error')
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="rounded-xl p-4 space-y-3" style={cardStyle}>
      <h3 className="text-sm font-semibold" style={{ color: 'var(--text-primary)' }}>{item ? 'Editar ítem' : 'Nuevo ítem de presupuesto'}</h3>
      <div className="grid gap-3 sm:grid-cols-3">
        <label className="text-xs" style={{ color: 'var(--text-secondary)' }}>Categoría
          <select value={category} onChange={(e) => setCategory(e.target.value as typeof category)} className="block w-full mt-1 px-2 py-1.5 rounded-lg text-sm outline-none" style={inputStyle}>
            {BUDGET_CATEGORIES.map((c) => <option key={c} value={c}>{CATEGORY_LABEL[c]}</option>)}
          </select>
        </label>
        <label className="text-xs sm:col-span-1" style={{ color: 'var(--text-secondary)' }}>Nombre
          <input value={name} onChange={(e) => setName(e.target.value)} maxLength={120} className="block w-full mt-1 px-2 py-1.5 rounded-lg text-sm outline-none" style={inputStyle} />
        </label>
        <label className="text-xs" style={{ color: 'var(--text-secondary)' }}>Monto aprobado
          <input type="number" min={0} value={approved} onChange={(e) => setApproved(e.target.value)} className="block w-full mt-1 px-2 py-1.5 rounded-lg text-sm outline-none" style={inputStyle} />
        </label>
      </div>
      <label className="text-xs block" style={{ color: 'var(--text-secondary)' }}>Notas (opcional)
        <input value={notes} onChange={(e) => setNotes(e.target.value)} maxLength={500} className="block w-full mt-1 px-2 py-1.5 rounded-lg text-sm outline-none" style={inputStyle} />
      </label>
      {amountChanged && (
        <label className="text-xs block" style={{ color: 'var(--text-secondary)' }}>Motivo del cambio de monto (obligatorio; queda en la bitácora con antes y después)
          <input value={reason} onChange={(e) => setReason(e.target.value)} maxLength={300} className="block w-full mt-1 px-2 py-1.5 rounded-lg text-sm outline-none" style={inputStyle} />
        </label>
      )}
      <div className="flex gap-2">
        <button disabled={busy || !name.trim() || approved === '' || (amountChanged && !reason.trim())} onClick={submit}
          className="px-3 py-1.5 rounded-lg text-sm text-white" style={{ backgroundColor: 'var(--accent)', opacity: busy || !name.trim() || approved === '' || (amountChanged && !reason.trim()) ? 0.5 : 1 }}>
          Guardar
        </button>
        <button onClick={onCancel} className="px-3 py-1.5 rounded-lg text-sm" style={inputStyle}>Cancelar</button>
      </div>
    </div>
  )
}

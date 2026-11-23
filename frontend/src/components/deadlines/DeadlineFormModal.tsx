import { useEffect, useState } from 'react'
import { Loader2, X } from 'lucide-react'
import {
  createDeadline,
  DEADLINE_CATEGORIES,
  fetchPortfolio,
  localDate,
  updateDeadline,
  type Deadline,
  type DeadlineCategory,
  type DeadlineInput,
  type DeadlineNotify,
} from '../../api'
import { useAuth } from '../../auth/AuthContext'
import { useToast } from '../ToastContext'

const inputStyle = { backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }
const labelStyle = { color: 'var(--text-secondary)' }

interface Props {
  deadline?: Deadline | null
  onClose: () => void
  onSaved: (d: Deadline) => void
}

/** Crear o editar un contador suelto */
export default function DeadlineFormModal({ deadline, onClose, onSaved }: Props) {
  const { addToast } = useToast()
  const { isModuleEnabled } = useAuth()
  const [projects, setProjects] = useState<{ id: string; title: string }[]>([])
  const [saving, setSaving] = useState(false)
  const [form, setForm] = useState<DeadlineInput>(() => ({
    title: deadline?.title ?? '',
    category: (deadline?.category === 'hito' ? 'otro' : deadline?.category) ?? 'informe',
    due_date: deadline?.due_date ?? localDate(new Date(Date.now() + 30 * 86400000)),
    notes: deadline?.notes ?? '',
    notify: deadline?.notify ?? 'creador',
    portfolio_id: deadline?.portfolio_id ?? null,
  }))

  useEffect(() => {
    if (!isModuleEnabled('portfolio')) return
    fetchPortfolio()
      .then((list) => setProjects(list.filter((p) => p.scope === 'own').map((p) => ({ id: p.id, title: p.title }))))
      .catch(() => {})
  }, [isModuleEnabled])

  const set = <K extends keyof DeadlineInput>(k: K, v: DeadlineInput[K]) => setForm((f) => ({ ...f, [k]: v }))

  const save = async () => {
    if (!form.title.trim()) {
      addToast('Falta el nombre', 'error')
      return
    }
    if (!form.due_date) {
      addToast('Falta la fecha', 'error')
      return
    }
    setSaving(true)
    try {
      const d = deadline?.id ? await updateDeadline(deadline.id, form) : await createDeadline(form)
      addToast(deadline ? 'Plazo guardado' : 'Plazo creado', 'success')
      onSaved(d)
    } catch (e) {
      addToast((e as Error).message, 'error')
    } finally {
      setSaving(false)
    }
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50" onClick={onClose}>
      <div
        className="rounded-xl p-6 w-full max-w-lg mx-4 max-h-[90vh] overflow-y-auto"
        style={{ backgroundColor: 'var(--bg-secondary)', border: '1px solid var(--border)' }}
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center justify-between mb-4">
          <h3 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>
            {deadline ? 'Editar plazo' : 'Nuevo plazo'}
          </h3>
          <button onClick={onClose} style={{ color: 'var(--text-secondary)' }}><X size={18} /></button>
        </div>

        <div className="space-y-3 text-sm">
          <label className="block">
            <span className="text-xs" style={labelStyle}>Nombre *</span>
            <input value={form.title} onChange={(e) => set('title', e.target.value)} maxLength={120} placeholder="Informe técnico de avance"
              className="w-full mt-1 px-3 py-2 rounded-lg outline-none" style={inputStyle} autoFocus />
          </label>
          <div className="grid grid-cols-2 gap-3">
            <label className="block">
              <span className="text-xs" style={labelStyle}>Tipo</span>
              <select value={form.category} onChange={(e) => set('category', e.target.value as DeadlineCategory)}
                className="w-full mt-1 px-3 py-2 rounded-lg outline-none" style={inputStyle}>
                {DEADLINE_CATEGORIES.filter((c) => c.id !== 'hito').map((c) => <option key={c.id} value={c.id}>{c.emoji} {c.label}</option>)}
              </select>
            </label>
            <label className="block">
              <span className="text-xs" style={labelStyle}>Fecha límite *</span>
              <input type="date" value={form.due_date} onChange={(e) => set('due_date', e.target.value)}
                className="w-full mt-1 px-3 py-2 rounded-lg outline-none" style={inputStyle} />
            </label>
          </div>
          {projects.length > 0 && (
            <label className="block">
              <span className="text-xs" style={labelStyle}>Proyecto (opcional)</span>
              <select value={form.portfolio_id ?? ''} onChange={(e) => set('portfolio_id', e.target.value || null)}
                className="w-full mt-1 px-3 py-2 rounded-lg outline-none" style={inputStyle}>
                <option value="">(ninguno)</option>
                {projects.map((p) => <option key={p.id} value={p.id}>{p.title}</option>)}
              </select>
            </label>
          )}
          <label className="block">
            <span className="text-xs" style={labelStyle}>Avisos por Telegram (30, 7 y 1 día antes, y el mismo día)</span>
            <select value={form.notify} onChange={(e) => set('notify', e.target.value as DeadlineNotify)}
              className="w-full mt-1 px-3 py-2 rounded-lg outline-none" style={inputStyle}>
              <option value="creador">Solo a mí</option>
              <option value="staff">A mí y a operadores y administradores</option>
              <option value="todos">A todo el equipo</option>
            </select>
          </label>
          <label className="block">
            <span className="text-xs" style={labelStyle}>Notas</span>
            <textarea value={form.notes} onChange={(e) => set('notes', e.target.value)} rows={3} maxLength={2000}
              placeholder="Qué hay que entregar, a quién, dónde..."
              className="w-full mt-1 px-3 py-2 rounded-lg outline-none resize-y" style={inputStyle} />
          </label>
        </div>

        <div className="flex justify-end gap-2 mt-5">
          <button onClick={onClose} className="px-4 py-2 rounded-lg text-sm" style={{ color: 'var(--text-secondary)' }}>Cancelar</button>
          <button onClick={save} disabled={saving} className="flex items-center gap-2 px-4 py-2 rounded-lg text-sm font-medium"
            style={{ backgroundColor: 'var(--accent)', color: '#fff', opacity: saving ? 0.7 : 1 }}>
            {saving && <Loader2 size={14} className="animate-spin" />} Guardar
          </button>
        </div>
      </div>
    </div>
  )
}

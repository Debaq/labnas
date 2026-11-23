import { useEffect, useState } from 'react'
import { Loader2, X } from 'lucide-react'
import {
  createMachine,
  fetchUsernames,
  MACHINE_KINDS,
  updateMachine,
  type Machine,
  type MachineInput,
  type MachineKind,
  type MachineStatus,
} from '../../api'
import { useToast } from '../ToastContext'

const inputStyle = { backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }
const labelStyle = { color: 'var(--text-secondary)' }

interface Props {
  machine?: Machine | null
  onClose: () => void
  onSaved: (m: Machine) => void
}

/** Crear o editar una maquina (admin u operador) */
export default function MachineFormModal({ machine, onClose, onSaved }: Props) {
  const { addToast } = useToast()
  const [users, setUsers] = useState<string[]>([])
  const [saving, setSaving] = useState(false)
  const [form, setForm] = useState<MachineInput>(() => ({
    name: machine?.name ?? '',
    kind: machine?.kind ?? 'laser',
    location: machine?.location ?? '',
    description: machine?.description ?? '',
    instructions: machine?.instructions ?? '',
    responsible: machine?.responsible ?? null,
    status: machine?.status ?? 'disponible',
    requires_approval: machine?.requires_approval ?? false,
    requires_certification: machine?.requires_certification ?? false,
    max_hours: machine?.max_hours ?? null,
    inventory_item_id: machine?.inventory_item_id ?? null,
    printer3d_id: machine?.printer3d_id ?? null,
  }))

  useEffect(() => {
    fetchUsernames().then(setUsers).catch(() => {})
  }, [])

  const set = <K extends keyof MachineInput>(k: K, v: MachineInput[K]) => setForm((f) => ({ ...f, [k]: v }))

  const save = async () => {
    if (!form.name.trim()) {
      addToast('Falta el nombre', 'error')
      return
    }
    setSaving(true)
    try {
      const m = machine ? await updateMachine(machine.id, form) : await createMachine(form)
      addToast(machine ? 'Maquina guardada' : 'Maquina creada', 'success')
      onSaved(m)
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
            {machine ? 'Editar maquina' : 'Nueva maquina'}
          </h3>
          <button onClick={onClose} style={{ color: 'var(--text-secondary)' }}><X size={18} /></button>
        </div>

        <div className="space-y-3 text-sm">
          <label className="block">
            <span className="text-xs" style={labelStyle}>Nombre *</span>
            <input value={form.name} onChange={(e) => set('name', e.target.value)} maxLength={100} placeholder="Laser CO2 60W"
              className="w-full mt-1 px-3 py-2 rounded-lg outline-none" style={inputStyle} autoFocus />
          </label>
          <div className="grid grid-cols-2 gap-3">
            <label className="block">
              <span className="text-xs" style={labelStyle}>Tipo</span>
              <select value={form.kind} onChange={(e) => set('kind', e.target.value as MachineKind)}
                className="w-full mt-1 px-3 py-2 rounded-lg outline-none" style={inputStyle}>
                {MACHINE_KINDS.map((k) => <option key={k.id} value={k.id}>{k.emoji} {k.label}</option>)}
              </select>
            </label>
            <label className="block">
              <span className="text-xs" style={labelStyle}>Ubicacion</span>
              <input value={form.location} onChange={(e) => set('location', e.target.value)} maxLength={200} placeholder="Sala 2"
                className="w-full mt-1 px-3 py-2 rounded-lg outline-none" style={inputStyle} />
            </label>
          </div>
          <div className="grid grid-cols-2 gap-3">
            <label className="block">
              <span className="text-xs" style={labelStyle}>Responsable</span>
              <select value={form.responsible ?? ''} onChange={(e) => set('responsible', e.target.value || null)}
                className="w-full mt-1 px-3 py-2 rounded-lg outline-none" style={inputStyle}>
                <option value="">(nadie)</option>
                {users.map((u) => <option key={u} value={u}>{u}</option>)}
              </select>
            </label>
            <label className="block">
              <span className="text-xs" style={labelStyle}>Maximo por reserva (horas)</span>
              <input type="number" min={0.5} max={336} step={0.5} value={form.max_hours ?? ''} placeholder="sin limite"
                onChange={(e) => set('max_hours', e.target.value ? Number(e.target.value) : null)}
                className="w-full mt-1 px-3 py-2 rounded-lg outline-none" style={inputStyle} />
            </label>
          </div>
          {machine && (
            <label className="block">
              <span className="text-xs" style={labelStyle}>Estado</span>
              <select value={form.status} onChange={(e) => set('status', e.target.value as MachineStatus)}
                className="w-full mt-1 px-3 py-2 rounded-lg outline-none" style={inputStyle}>
                <option value="disponible">Disponible</option>
                <option value="fuera_servicio">Fuera de servicio</option>
                <option value="retirada">Retirada</option>
              </select>
            </label>
          )}
          <label className="flex items-center gap-2 cursor-pointer" style={{ color: 'var(--text-primary)' }}>
            <input type="checkbox" checked={form.requires_approval} onChange={(e) => set('requires_approval', e.target.checked)} />
            Las reservas requieren aprobacion de un operador
          </label>
          <label className="flex items-center gap-2 cursor-pointer" style={{ color: 'var(--text-primary)' }}>
            <input type="checkbox" checked={form.requires_certification} onChange={(e) => set('requires_certification', e.target.checked)} />
            Exigir induccion para reservar (operadores y administradores no la necesitan)
          </label>
          <label className="block">
            <span className="text-xs" style={labelStyle}>Descripcion</span>
            <textarea value={form.description} onChange={(e) => set('description', e.target.value)} rows={2} maxLength={5000}
              placeholder="Area de trabajo, potencia, materiales que acepta..."
              className="w-full mt-1 px-3 py-2 rounded-lg outline-none resize-y" style={inputStyle} />
          </label>
          <label className="block">
            <span className="text-xs" style={labelStyle}>Instrucciones de uso y seguridad</span>
            <textarea value={form.instructions} onChange={(e) => set('instructions', e.target.value)} rows={4} maxLength={5000}
              placeholder={'1. Encender el extractor\n2. Nunca dejar la laser sola mientras corta\n...'}
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

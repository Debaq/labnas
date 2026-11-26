import { useCallback, useEffect, useState } from 'react'
import { Camera, Check, Plus, Scissors, Trash2 } from 'lucide-react'
import { compressImage, createOffcut, deleteOffcut, fetchOffcuts, offcutPhotoUrl, takeOffcut, type Offcut } from '../../api'
import { useAuth } from '../../auth/AuthContext'
import { useEvent } from '../../events/useEvents'
import { useToast } from '../ToastContext'

const input = { backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }
const num = (v: string) => (v.trim() === '' ? null : Number(v.replace(',', '.')))

/** Retazos de plancha: buscar uno que sirva antes de cortar una plancha nueva */
export default function OffcutsPanel() {
  const { user, canModule } = useAuth()
  const { addToast } = useToast()
  const staff = canModule('inventory', 'gestionar')
  const canAdd = canModule('inventory', 'usar')
  const [list, setList] = useState<Offcut[]>([])
  const [filter, setFilter] = useState<{ material: string; thickness: number | null; min_w: number | null; min_h: number | null }>({ material: '', thickness: null, min_w: null, min_h: null })
  const [form, setForm] = useState<{ material: string; thickness_mm: number | null; width_mm: number | null; height_mm: number | null; location: string; notes: string; photo?: string } | null>(null)

  const load = useCallback(() => { fetchOffcuts(filter).then(setList).catch(() => {}) }, [filter])
  useEffect(() => { const t = setTimeout(load, 250); return () => clearTimeout(t) }, [load])
  useEvent('inventory.offcuts', load, load)

  async function run(fn: () => Promise<unknown>, ok: string) {
    try { await fn(); addToast(ok, 'success'); load() } catch (e) { addToast((e as Error).message, 'error') }
  }

  async function save() {
    if (!form || !form.width_mm || !form.height_mm) return
    await run(() => createOffcut({ ...form, width_mm: form.width_mm!, height_mm: form.height_mm! }), 'Retazo guardado')
    setForm(null)
  }

  return (
    <div className="space-y-3">
      <div className="rounded-xl p-4 space-y-2" style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}>
        <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>
          Antes de cortar una plancha nueva, busca un retazo que sirva. Indica la pieza que necesitas: se muestran los mas chicos que alcanzan (se pueden girar).
        </p>
        <div className="grid grid-cols-2 md:grid-cols-5 gap-2">
          <input value={filter.material} onChange={e => setFilter({ ...filter, material: e.target.value })} placeholder="Material" className="col-span-2 md:col-span-1 rounded-lg px-2 py-1.5 text-sm" style={input} />
          <input type="number" step="0.1" value={filter.thickness ?? ''} onChange={e => setFilter({ ...filter, thickness: num(e.target.value) })} placeholder="Espesor mm" className="rounded-lg px-2 py-1.5 text-sm" style={input} />
          <input type="number" value={filter.min_w ?? ''} onChange={e => setFilter({ ...filter, min_w: num(e.target.value) })} placeholder="Pieza ancho mm" className="rounded-lg px-2 py-1.5 text-sm" style={input} />
          <input type="number" value={filter.min_h ?? ''} onChange={e => setFilter({ ...filter, min_h: num(e.target.value) })} placeholder="Pieza alto mm" className="rounded-lg px-2 py-1.5 text-sm" style={input} />
          {canAdd && !form && (
            <button onClick={() => setForm({ material: filter.material, thickness_mm: filter.thickness, width_mm: null, height_mm: null, location: '', notes: '' })}
              className="flex items-center justify-center gap-1 rounded-lg text-sm" style={{ backgroundColor: 'var(--accent)', color: '#fff' }}>
              <Plus size={14} /> Retazo
            </button>
          )}
        </div>
      </div>

      {form && (
        <div className="rounded-xl p-4 space-y-2" style={{ border: '1px solid var(--border)', backgroundColor: 'var(--card-bg)' }}>
          <div className="grid grid-cols-2 md:grid-cols-4 gap-2">
            <input value={form.material} onChange={e => setForm({ ...form, material: e.target.value })} placeholder="Material: MDF, acrilico…" className="col-span-2 rounded-lg px-2 py-1.5 text-sm" style={input} />
            <input type="number" step="0.1" value={form.thickness_mm ?? ''} onChange={e => setForm({ ...form, thickness_mm: num(e.target.value) })} placeholder="Espesor mm" className="rounded-lg px-2 py-1.5 text-sm" style={input} />
            <input value={form.location} onChange={e => setForm({ ...form, location: e.target.value })} placeholder="Donde esta" className="rounded-lg px-2 py-1.5 text-sm" style={input} />
            <input type="number" value={form.width_mm ?? ''} onChange={e => setForm({ ...form, width_mm: num(e.target.value) })} placeholder="Ancho mm" className="rounded-lg px-2 py-1.5 text-sm" style={input} />
            <input type="number" value={form.height_mm ?? ''} onChange={e => setForm({ ...form, height_mm: num(e.target.value) })} placeholder="Alto mm" className="rounded-lg px-2 py-1.5 text-sm" style={input} />
            <input value={form.notes} onChange={e => setForm({ ...form, notes: e.target.value })} placeholder="Notas (forma irregular, rayado…)" className="col-span-2 rounded-lg px-2 py-1.5 text-sm" style={input} />
          </div>
          <div className="flex items-center justify-between">
            <label className="flex items-center gap-1 text-xs cursor-pointer" style={{ color: form.photo ? 'var(--success)' : 'var(--text-secondary)' }}>
              <Camera size={14} /> {form.photo ? 'Foto lista' : 'Foto (opcional)'}
              <input type="file" accept="image/*" capture="environment" className="hidden" onChange={async e => { const f = e.target.files?.[0]; if (f) setForm({ ...form, photo: await compressImage(f) }) }} />
            </label>
            <div className="flex gap-2">
              <button onClick={() => setForm(null)} className="px-3 py-1.5 rounded-lg text-sm" style={{ color: 'var(--text-secondary)' }}>Cancelar</button>
              <button onClick={save} disabled={!form.material.trim() || !form.width_mm || !form.height_mm} className="px-3 py-1.5 rounded-lg text-sm disabled:opacity-50" style={{ backgroundColor: 'var(--accent)', color: '#fff' }}>Guardar</button>
            </div>
          </div>
        </div>
      )}

      {list.length === 0 && <p className="text-sm text-center py-8" style={{ color: 'var(--text-secondary)' }}>No hay retazos que sirvan.</p>}
      <div className="grid gap-3 grid-cols-1 sm:grid-cols-2 lg:grid-cols-3">
        {list.map(o => (
          <div key={o.id} className="rounded-xl overflow-hidden" style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}>
            {o.has_photo ? (
              <img src={offcutPhotoUrl(o.id)} alt="" className="w-full h-32 object-cover" loading="lazy" />
            ) : (
              <div className="w-full h-16 flex items-center justify-center" style={{ backgroundColor: 'var(--bg-secondary)' }}><Scissors size={20} style={{ color: 'var(--text-secondary)' }} /></div>
            )}
            <div className="p-3 space-y-1">
              <div className="text-sm font-medium" style={{ color: 'var(--text-primary)' }}>
                {o.material}{o.thickness_mm != null && ` ${o.thickness_mm} mm`}
              </div>
              <div className="text-sm font-mono" style={{ color: 'var(--text-primary)' }}>{o.width_mm} × {o.height_mm} mm</div>
              <div className="text-xs" style={{ color: 'var(--text-secondary)' }}>
                {o.location && `${o.location} · `}{o.added_by}{o.notes && ` · ${o.notes}`}
              </div>
              <div className="flex justify-between pt-1">
                <button onClick={() => run(() => takeOffcut(o.id), 'Marcado como tomado')} className="flex items-center gap-1 text-xs" style={{ color: 'var(--accent)' }}>
                  <Check size={13} /> Lo tome
                </button>
                {(staff || o.added_by === user?.username) && (
                  <button onClick={() => confirm('Borrar este retazo?') && run(() => deleteOffcut(o.id), 'Borrado')} className="p-1" title="Borrar"><Trash2 size={13} style={{ color: 'var(--danger)' }} /></button>
                )}
              </div>
            </div>
          </div>
        ))}
      </div>
    </div>
  )
}

import { useCallback, useEffect, useState } from 'react'
import { Plus, Recycle, Trash2 } from 'lucide-react'
import { addRecycling, deleteRecycling, fetchRecycling, type RecyclingKind, type RecyclingSummary } from '../../api'
import { useAuth } from '../../auth/AuthContext'
import { useToast } from '../ToastContext'

const card = { backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }
const input = { backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }
const KIND: Record<RecyclingKind, { label: string; color: string }> = {
  triturado: { label: 'Triturado', color: 'var(--accent)' },
  recuperado: { label: 'Filamento recuperado', color: 'var(--success)' },
  desecho: { label: 'Desecho', color: 'var(--danger)' },
}
const today = () => new Date().toLocaleDateString('sv-SE')
const kg = (n: number) => `${Math.round(n * 100) / 100} kg`

/** Reciclaje: kilos triturados, recuperados y desechados, por mes y material */
export default function RecyclingPanel() {
  const { user, canModule } = useAuth()
  const { addToast } = useToast()
  const staff = canModule('inventory', 'gestionar')
  const canAdd = canModule('inventory', 'usar')
  const [data, setData] = useState<RecyclingSummary | null>(null)
  const [form, setForm] = useState<{ date: string; material: string; kind: RecyclingKind; kg: string; notes: string } | null>(null)

  const load = useCallback(() => { fetchRecycling().then(setData).catch(() => {}) }, [])
  useEffect(() => { load() }, [load])

  async function run(fn: () => Promise<unknown>, ok: string) {
    try { await fn(); addToast(ok, 'success'); load() } catch (e) { addToast((e as Error).message, 'error') }
  }

  if (!data) return null
  const months = [...new Set(data.by_month.map(([m]) => m))]
  const monthMax = Math.max(1, ...months.map(m => data.by_month.filter(([x]) => x === m).reduce((s, [, , v]) => s + v, 0)))
  const processed = data.total_triturado + data.total_recuperado + data.total_desecho

  return (
    <div className="space-y-3">
      <div className="grid grid-cols-3 gap-3">
        {(['triturado', 'recuperado', 'desecho'] as RecyclingKind[]).map(k => (
          <div key={k} className="rounded-xl p-4" style={card}>
            <div className="text-xs" style={{ color: 'var(--text-secondary)' }}>{KIND[k].label} · 12 meses</div>
            <div className="text-2xl font-bold" style={{ color: KIND[k].color }}>{kg(k === 'triturado' ? data.total_triturado : k === 'recuperado' ? data.total_recuperado : data.total_desecho)}</div>
          </div>
        ))}
      </div>
      {processed > 0 && (
        <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>
          Se reaprovecho el {Math.round(((data.total_triturado + data.total_recuperado) / processed) * 100)}% de lo registrado en el ultimo año.
        </p>
      )}

      <div className="rounded-xl p-4 space-y-3" style={card}>
        <div className="flex items-center justify-between">
          <h2 className="text-sm font-semibold flex items-center gap-2" style={{ color: 'var(--text-primary)' }}><Recycle size={16} /> Movimientos</h2>
          {canAdd && !form && (
            <button onClick={() => setForm({ date: today(), material: 'PLA', kind: 'triturado', kg: '', notes: '' })} className="flex items-center gap-1 px-2 py-1 rounded-lg text-xs" style={{ backgroundColor: 'var(--accent)', color: '#fff' }}>
              <Plus size={13} /> Anotar
            </button>
          )}
        </div>
        {form && (
          <div className="grid grid-cols-2 md:grid-cols-5 gap-2">
            <input type="date" max={today()} value={form.date} onChange={e => setForm({ ...form, date: e.target.value })} className="rounded-lg px-2 py-1.5 text-sm" style={input} />
            <input value={form.material} onChange={e => setForm({ ...form, material: e.target.value })} placeholder="Material" className="rounded-lg px-2 py-1.5 text-sm" style={input} />
            <select value={form.kind} onChange={e => setForm({ ...form, kind: e.target.value as RecyclingKind })} className="rounded-lg px-2 py-1.5 text-sm" style={input}>
              {(Object.keys(KIND) as RecyclingKind[]).map(k => <option key={k} value={k}>{KIND[k].label}</option>)}
            </select>
            <input type="number" min={0} step="0.01" value={form.kg} onChange={e => setForm({ ...form, kg: e.target.value })} placeholder="Kilos" className="rounded-lg px-2 py-1.5 text-sm" style={input} />
            <input value={form.notes} onChange={e => setForm({ ...form, notes: e.target.value })} placeholder="Notas" className="rounded-lg px-2 py-1.5 text-sm" style={input} />
            <div className="col-span-full flex justify-end gap-2">
              <button onClick={() => setForm(null)} className="px-3 py-1 rounded-lg text-sm" style={{ color: 'var(--text-secondary)' }}>Cancelar</button>
              <button onClick={() => run(() => addRecycling({ ...form, kg: Number(form.kg.replace(',', '.')) }), 'Anotado').then(() => setForm(null))} className="px-3 py-1 rounded-lg text-sm" style={{ backgroundColor: 'var(--accent)', color: '#fff' }}>Guardar</button>
            </div>
          </div>
        )}
        {months.length > 0 && (
          <div className="flex items-end gap-2 h-28">
            {months.map(m => {
              const rows = data.by_month.filter(([x]) => x === m)
              return (
                <div key={m} className="flex-1 flex flex-col items-center gap-1">
                  <div className="w-full flex flex-col-reverse" style={{ height: 88 }}>
                    {rows.map(([, k, v]) => (
                      <div key={k} title={`${KIND[k].label}: ${kg(v)}`} style={{ height: `${(v / monthMax) * 88}px`, backgroundColor: KIND[k].color }} />
                    ))}
                  </div>
                  <span className="text-[10px]" style={{ color: 'var(--text-secondary)' }}>{m.slice(5)}/{m.slice(2, 4)}</span>
                </div>
              )
            })}
          </div>
        )}
        {data.entries.length === 0 && <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>Sin movimientos. Anota cada vez que se tritura, se recupera filamento o se bota plastico.</p>}
        <ul className="space-y-1">
          {data.entries.map(e => (
            <li key={e.id} className="flex items-center justify-between gap-2 text-xs px-2 py-1 rounded" style={{ backgroundColor: 'var(--bg-secondary)', color: 'var(--text-primary)' }}>
              <span>{e.date} · <b>{kg(e.kg)}</b> de {e.material} · <span style={{ color: KIND[e.kind].color }}>{KIND[e.kind].label}</span>{e.notes && ` · ${e.notes}`} · <span style={{ color: 'var(--text-secondary)' }}>{e.created_by}</span></span>
              {(staff || e.created_by === user?.username) && (
                <button onClick={() => confirm('Borrar este registro?') && run(() => deleteRecycling(e.id), 'Borrado')} className="p-1"><Trash2 size={12} style={{ color: 'var(--danger)' }} /></button>
              )}
            </li>
          ))}
        </ul>
      </div>
    </div>
  )
}

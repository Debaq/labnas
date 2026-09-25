import { useCallback, useEffect, useState } from 'react'
import { ListOrdered, X, Plus, Loader2, ArrowUp, ArrowDown, Play, Check, Ban, Trash2, FileCode } from 'lucide-react'
import {
  fetchPrintQueue, addToPrintQueue, updatePrintQueueItem, deletePrintQueueItem, reorderPrintQueue,
  type QueueItem, type QueueInput,
} from '../../api'
import type { Printer3DConfig } from '../../types'
import { useAuth } from '../../auth/AuthContext'
import { useEvent } from '../../events/useEvents'
import { readGcodeInfo } from '../../lib/gcode'
import { errorMessage } from '../../lib/errors'

const STATUS: Record<QueueItem['status'], { label: string; color: string }> = {
  pendiente: { label: 'Pendiente', color: 'var(--text-secondary)' },
  imprimiendo: { label: 'Imprimiendo', color: 'var(--accent)' },
  terminado: { label: 'Terminado', color: 'var(--success)' },
  cancelado: { label: 'Cancelado', color: 'var(--danger)' },
}

const EMPTY: QueueInput = { title: '', file_name: '', notes: '', printer_id: null, grams: null, seconds: null }
const inputStyle = { backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }

function duration(seconds: number | null): string {
  if (!seconds) return ''
  const h = Math.floor(seconds / 3600)
  const m = Math.round((seconds % 3600) / 60)
  return h > 0 ? `${h}h ${m}m` : `${m}m`
}

/** Cola compartida: cualquiera pide; operadores y admins ordenan, asignan y marcan estados */
export default function PrintQueueModal({ printers, onClose }: { printers: Printer3DConfig[]; onClose: () => void }) {
  const { user } = useAuth()
  const manager = user?.role === 'admin' || user?.role === 'operador'
  const [items, setItems] = useState<QueueItem[] | null>(null)
  const [form, setForm] = useState<QueueInput | null>(null)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const load = useCallback(() => {
    fetchPrintQueue().then(setItems).catch((e) => setError(errorMessage(e)))
  }, [])
  useEffect(() => { load() }, [load])
  useEvent('printers3d.queue', load, load)

  async function run(fn: () => Promise<unknown>) {
    setError(null)
    try { await fn(); load() } catch (e) { setError(errorMessage(e)) }
  }

  async function submit() {
    if (!form) return
    setBusy(true)
    try { await addToPrintQueue(form); setForm(null); load() } catch (e) { setError(errorMessage(e)) } finally { setBusy(false) }
  }

  async function loadGcode(e: React.ChangeEvent<HTMLInputElement>) {
    const file = e.target.files?.[0]
    e.target.value = ''
    if (!file || !form) return
    const info = await readGcodeInfo(file)
    setForm({
      ...form,
      file_name: file.name,
      title: form.title || file.name.replace(/\.(gcode|gco|g|bgcode)$/i, ''),
      grams: info.grams ?? form.grams ?? null,
      seconds: info.seconds ?? form.seconds ?? null,
    })
  }

  const pending = items?.filter((i) => i.status === 'pendiente') ?? []
  function move(id: string, dir: -1 | 1) {
    const ids = pending.map((i) => i.id)
    const k = ids.indexOf(id)
    const j = k + dir
    if (k < 0 || j < 0 || j >= ids.length) return
    ;[ids[k], ids[j]] = [ids[j], ids[k]]
    run(() => reorderPrintQueue(ids))
  }

  const printerName = (id: string | null) => printers.find((p) => p.id === id)?.name

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4" style={{ backgroundColor: 'rgba(0,0,0,0.5)' }} onClick={onClose}>
      <div className="w-full max-w-3xl max-h-[85vh] flex flex-col rounded-xl" style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }} onClick={(e) => e.stopPropagation()}>
        <div className="flex items-center justify-between p-4" style={{ borderBottom: '1px solid var(--border)' }}>
          <div className="flex items-center gap-2">
            <ListOrdered size={18} style={{ color: 'var(--accent)' }} />
            <h2 className="font-semibold" style={{ color: 'var(--text-primary)' }}>Cola de impresion</h2>
          </div>
          <div className="flex items-center gap-2">
            {!form && (
              <button onClick={() => setForm({ ...EMPTY })} className="flex items-center gap-1 px-3 py-1.5 rounded-lg text-xs" style={{ backgroundColor: 'var(--accent)', color: '#fff' }}>
                <Plus size={13} /> Pedir impresion
              </button>
            )}
            <button onClick={onClose} style={{ color: 'var(--text-secondary)' }}><X size={18} /></button>
          </div>
        </div>

        {error && <p className="px-4 pt-3 text-sm" style={{ color: 'var(--danger)' }}>{error}</p>}

        <div className="overflow-auto p-4 space-y-3">
          {form && (
            <div className="rounded-lg p-4 space-y-2" style={{ backgroundColor: 'var(--bg-tertiary)' }}>
              <div className="grid grid-cols-1 md:grid-cols-2 gap-2">
                <input value={form.title} onChange={(e) => setForm({ ...form, title: e.target.value })} placeholder="Que hay que imprimir" className="px-3 py-2 rounded-lg text-sm outline-none" style={inputStyle} />
                <select value={form.printer_id ?? ''} onChange={(e) => setForm({ ...form, printer_id: e.target.value || null })} className="px-3 py-2 rounded-lg text-sm outline-none" style={inputStyle}>
                  <option value="">Cualquier impresora</option>
                  {printers.map((p) => <option key={p.id} value={p.id}>{p.name}</option>)}
                </select>
              </div>
              <textarea value={form.notes} onChange={(e) => setForm({ ...form, notes: e.target.value })} placeholder="Notas: material, color, para cuando..." rows={2} className="w-full px-3 py-2 rounded-lg text-sm outline-none" style={inputStyle} />
              <div className="flex items-center justify-between gap-2 flex-wrap">
                <label className="flex items-center gap-1 px-3 py-1.5 rounded-lg text-xs cursor-pointer" style={{ color: 'var(--accent)', border: '1px dashed var(--border)' }}>
                  <FileCode size={12} /> {form.file_name || 'Adjuntar gcode (estimacion)'}
                  <input type="file" accept=".gcode,.gco,.g,.bgcode" className="hidden" onChange={loadGcode} />
                </label>
                <span className="text-xs" style={{ color: 'var(--text-secondary)' }}>
                  {[form.grams ? `${form.grams.toFixed(1)} g` : '', duration(form.seconds ?? null)].filter(Boolean).join(' · ')}
                </span>
                <div className="flex gap-2">
                  <button onClick={() => setForm(null)} className="px-3 py-1.5 rounded-lg text-xs" style={{ color: 'var(--text-secondary)', border: '1px solid var(--border)' }}>Cancelar</button>
                  <button onClick={submit} disabled={busy || !form.title.trim()} className="flex items-center gap-1 px-3 py-1.5 rounded-lg text-xs disabled:opacity-50" style={{ backgroundColor: 'var(--accent)', color: '#fff' }}>
                    {busy && <Loader2 size={12} className="animate-spin" />} Enviar
                  </button>
                </div>
              </div>
            </div>
          )}

          {items === null && <Loader2 size={18} className="animate-spin" />}
          {items?.length === 0 && <p className="text-sm" style={{ color: 'var(--text-secondary)' }}>No hay pedidos.</p>}
          {items?.map((it) => {
            const mine = it.requested_by === user?.username
            const st = STATUS[it.status]
            const active = it.status === 'pendiente' || it.status === 'imprimiendo'
            return (
              <div key={it.id} className="flex items-center gap-3 px-3 py-2 rounded-lg" style={{ backgroundColor: 'var(--bg-tertiary)', opacity: active ? 1 : 0.6 }}>
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-2">
                    <span className="text-sm font-medium truncate" style={{ color: 'var(--text-primary)' }}>{it.title}</span>
                    <span className="text-[10px] px-1.5 rounded" style={{ color: st.color, border: `1px solid ${st.color}` }}>{st.label}</span>
                  </div>
                  <p className="text-xs truncate" style={{ color: 'var(--text-secondary)' }}>
                    {[it.requested_by, printerName(it.printer_id), it.file_name, it.grams ? `${it.grams.toFixed(1)} g` : '', duration(it.seconds)].filter(Boolean).join(' · ')}
                  </p>
                  {it.notes && <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>{it.notes}</p>}
                </div>
                <div className="flex items-center gap-1 shrink-0">
                  {manager && it.status === 'pendiente' && (
                    <>
                      <button title="Subir" onClick={() => move(it.id, -1)} className="p-1" style={{ color: 'var(--text-secondary)' }}><ArrowUp size={14} /></button>
                      <button title="Bajar" onClick={() => move(it.id, 1)} className="p-1" style={{ color: 'var(--text-secondary)' }}><ArrowDown size={14} /></button>
                      <button title="Imprimiendo" onClick={() => run(() => updatePrintQueueItem(it.id, { status: 'imprimiendo' }))} className="p-1" style={{ color: 'var(--accent)' }}><Play size={14} /></button>
                    </>
                  )}
                  {manager && it.status === 'imprimiendo' && (
                    <button title="Terminado" onClick={() => run(() => updatePrintQueueItem(it.id, { status: 'terminado' }))} className="p-1" style={{ color: 'var(--success)' }}><Check size={14} /></button>
                  )}
                  {active && (manager || (mine && it.status === 'pendiente')) && (
                    <button title="Cancelar" onClick={() => run(() => updatePrintQueueItem(it.id, { status: 'cancelado' }))} className="p-1" style={{ color: 'var(--warning)' }}><Ban size={14} /></button>
                  )}
                  {(manager || (mine && it.status === 'pendiente')) && (
                    <button title="Borrar" onClick={() => { if (confirm(`Borrar el pedido "${it.title}"?`)) run(() => deletePrintQueueItem(it.id)) }} className="p-1" style={{ color: 'var(--danger)' }}><Trash2 size={14} /></button>
                  )}
                </div>
              </div>
            )
          })}
        </div>
      </div>
    </div>
  )
}

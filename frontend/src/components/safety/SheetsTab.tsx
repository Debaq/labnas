import { useEffect, useState } from 'react'
import { ExternalLink, FileText, Loader2, Plus, Trash2 } from 'lucide-react'
import {
  createSheet,
  deleteSheet,
  fetchInventoryItems,
  fetchMachines,
  sheetFileUrl,
  updateSheet,
  type Machine,
  type SafetySheet,
  type SheetKind,
} from '../../api'
import type { InventoryItem } from '../../types'
import { useToast } from '../ToastContext'
import { errorMessage } from '../../lib/errors'
import { SHEET_KINDS, cardStyle, inputStyle, readAsDataUrl } from '../../lib/safety'

function SheetForm({ initial, machines, items, onDone, onCancel }: {
  initial: SafetySheet | null
  machines: Machine[]
  items: InventoryItem[]
  onDone: () => void
  onCancel: () => void
}) {
  const { addToast } = useToast()
  const [title, setTitle] = useState(initial?.title ?? '')
  const [kind, setKind] = useState<SheetKind>(initial?.kind ?? 'material')
  const [content, setContent] = useState(initial?.content ?? '')
  const [url, setUrl] = useState(initial?.url ?? '')
  const [machine, setMachine] = useState(initial?.machine_id ?? '')
  const [item, setItem] = useState(initial?.inventory_item_id ?? '')
  const [file, setFile] = useState<{ name: string; data: string } | null>(null)
  const [removeFile, setRemoveFile] = useState(false)
  const [busy, setBusy] = useState(false)

  const save = async () => {
    setBusy(true)
    try {
      const data = { title, kind, content, url, machine_id: machine || null, inventory_item_id: item || null, file, remove_file: removeFile }
      if (initial) await updateSheet(initial.id, data)
      else await createSheet(data)
      addToast('Ficha guardada', 'success')
      onDone()
    } catch (e) {
      addToast(errorMessage(e), 'error')
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="rounded-xl p-4 space-y-3" style={cardStyle}>
      <h3 className="text-sm font-semibold" style={{ color: 'var(--text-primary)' }}>{initial ? 'Editar ficha' : 'Nueva ficha'}</h3>
      <div className="grid gap-3 sm:grid-cols-2">
        <input value={title} onChange={(e) => setTitle(e.target.value)} maxLength={160} placeholder="Titulo" className="px-3 py-1.5 rounded-lg text-sm outline-none" style={inputStyle} />
        <select value={kind} onChange={(e) => setKind(e.target.value as SheetKind)} className="px-2 py-1.5 rounded-lg text-sm outline-none" style={inputStyle}>
          {(Object.keys(SHEET_KINDS) as SheetKind[]).map((k) => <option key={k} value={k}>{SHEET_KINDS[k]}</option>)}
        </select>
        <select value={machine} onChange={(e) => setMachine(e.target.value)} className="px-2 py-1.5 rounded-lg text-sm outline-none" style={inputStyle}>
          <option value="">Sin maquina vinculada</option>
          {machines.map((m) => <option key={m.id} value={m.id}>{m.name}</option>)}
        </select>
        <select value={item} onChange={(e) => setItem(e.target.value)} className="px-2 py-1.5 rounded-lg text-sm outline-none" style={inputStyle}>
          <option value="">Sin item de inventario vinculado</option>
          {items.map((i) => <option key={i.id} value={i.id}>{i.name}</option>)}
        </select>
      </div>
      <textarea value={content} onChange={(e) => setContent(e.target.value)} maxLength={20000} rows={4} placeholder="Texto del procedimiento o resumen de la ficha" className="w-full px-3 py-2 rounded-lg text-sm outline-none" style={inputStyle} />
      <input value={url} onChange={(e) => setUrl(e.target.value)} maxLength={500} placeholder="Enlace (https://... o ruta del NAS /...)" className="w-full px-3 py-1.5 rounded-lg text-sm outline-none" style={inputStyle} />
      <div className="flex items-center gap-3 flex-wrap text-xs" style={{ color: 'var(--text-secondary)' }}>
        <label className="px-3 py-1.5 rounded-lg cursor-pointer" style={inputStyle}>
          {file ? file.name : 'Adjuntar PDF o imagen (max 5 MB)'}
          <input type="file" accept="application/pdf,image/jpeg,image/png" className="hidden" onChange={async (e) => {
            const f = e.target.files?.[0]
            e.target.value = ''
            if (!f) return
            if (f.size > 5 * 1024 * 1024) { addToast('El archivo pesa mas de 5 MB', 'error'); return }
            setFile({ name: f.name, data: await readAsDataUrl(f) })
            setRemoveFile(false)
          }} />
        </label>
        {initial?.file_name && !file && (
          <label className="flex items-center gap-1.5">
            <input type="checkbox" checked={removeFile} onChange={(e) => setRemoveFile(e.target.checked)} /> Quitar el archivo actual ({initial.file_name})
          </label>
        )}
        <div className="ml-auto flex gap-2">
          <button onClick={onCancel} className="px-3 py-1.5 rounded-lg text-sm" style={inputStyle}>Cancelar</button>
          <button disabled={busy || !title.trim()} onClick={save} className="flex items-center gap-1.5 px-4 py-1.5 rounded-lg text-sm font-medium" style={{ backgroundColor: 'var(--accent)', color: '#fff' }}>
            {busy && <Loader2 size={14} className="animate-spin" />} Guardar
          </button>
        </div>
      </div>
    </div>
  )
}

interface Props {
  sheets: SafetySheet[]
  staff: boolean
  onChange: () => void
}

/** Fichas de seguridad y procedimientos: todos las ven, Gestionar las administra */
export default function SheetsTab({ sheets, staff, onChange }: Props) {
  const { addToast } = useToast()
  const [machines, setMachines] = useState<Machine[]>([])
  const [items, setItems] = useState<InventoryItem[]>([])
  const [editing, setEditing] = useState<SafetySheet | 'new' | null>(null)
  const [query, setQuery] = useState('')

  useEffect(() => {
    if (!staff) return
    fetchMachines().then(setMachines).catch(() => {})
    fetchInventoryItems().then(setItems).catch(() => {})
  }, [staff])

  const q = query.trim().toLowerCase()
  const shown = sheets.filter((s) => !q || `${s.title} ${s.content} ${s.machine_name ?? ''} ${s.inventory_item_name ?? ''}`.toLowerCase().includes(q))

  return (
    <div className="space-y-3">
      <div className="flex items-center gap-2">
        <input value={query} onChange={(e) => setQuery(e.target.value)} placeholder="Buscar ficha, material o maquina" className="flex-1 px-3 py-1.5 rounded-lg text-sm outline-none" style={inputStyle} />
        {staff && (
          <button onClick={() => setEditing('new')} className="flex items-center gap-1.5 px-4 py-1.5 rounded-lg text-sm font-medium" style={{ backgroundColor: 'var(--accent)', color: '#fff' }}>
            <Plus size={14} /> Nueva
          </button>
        )}
      </div>
      {editing && (
        <SheetForm initial={editing === 'new' ? null : editing} machines={machines} items={items}
          onDone={() => { setEditing(null); onChange() }} onCancel={() => setEditing(null)} />
      )}
      {shown.length === 0 && <p className="text-sm py-6 text-center" style={{ color: 'var(--text-secondary)' }}>No hay fichas.</p>}
      {shown.map((s) => (
        <div key={s.id} className="rounded-xl p-4 space-y-1.5" style={cardStyle}>
          <div className="flex items-center gap-2 flex-wrap">
            <span className="text-sm font-semibold" style={{ color: 'var(--text-primary)' }}>{s.title}</span>
            <span className="text-[11px] px-2 py-0.5 rounded-full" style={{ color: 'var(--text-secondary)', border: '1px solid currentColor' }}>{SHEET_KINDS[s.kind]}</span>
            {s.machine_name && <span className="text-xs" style={{ color: 'var(--text-secondary)' }}>Maquina: {s.machine_name}</span>}
            {s.inventory_item_name && <span className="text-xs" style={{ color: 'var(--text-secondary)' }}>Inventario: {s.inventory_item_name}</span>}
            {staff && (
              <div className="ml-auto flex gap-3 text-xs">
                <button onClick={() => setEditing(s)} style={{ color: 'var(--accent)' }}>Editar</button>
                <button aria-label="Eliminar" style={{ color: 'var(--danger)' }} onClick={async () => {
                  if (!confirm(`Eliminar la ficha "${s.title}"?`)) return
                  try { await deleteSheet(s.id); addToast('Ficha eliminada', 'success'); onChange() } catch (e) { addToast(errorMessage(e), 'error') }
                }}><Trash2 size={14} /></button>
              </div>
            )}
          </div>
          {s.content && <p className="text-sm whitespace-pre-wrap" style={{ color: 'var(--text-primary)' }}>{s.content}</p>}
          <div className="flex gap-4 text-xs flex-wrap">
            {s.file_name && (
              <a href={sheetFileUrl(s.id)} target="_blank" rel="noreferrer" className="flex items-center gap-1" style={{ color: 'var(--accent)' }}>
                <FileText size={13} /> {s.file_name}
              </a>
            )}
            {s.url && (
              <a href={s.url} target="_blank" rel="noreferrer noopener" className="flex items-center gap-1 break-all" style={{ color: 'var(--accent)' }}>
                <ExternalLink size={13} /> {s.url}
              </a>
            )}
          </div>
        </div>
      ))}
    </div>
  )
}

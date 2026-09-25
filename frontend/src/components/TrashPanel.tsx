import { useCallback, useEffect, useState } from 'react'
import { Trash2, RotateCcw, X, Loader2, Folder, File } from 'lucide-react'
import { fetchTrash, restoreTrashItem, deleteTrashItem, emptyTrash, type TrashItem } from '../api'

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  const units = ['KB', 'MB', 'GB', 'TB']
  let v = bytes / 1024
  let i = 0
  while (v >= 1024 && i < units.length - 1) { v /= 1024; i++ }
  return `${v.toFixed(1)} ${units[i]}`
}

interface Props {
  isAdmin: boolean
  onClose: () => void
  /** Tras restaurar, para refrescar el listado del explorador */
  onRestored: () => void
}

/** Papelera: restaurar o borrar definitivamente lo eliminado desde el explorador */
export default function TrashPanel({ isAdmin, onClose, onRestored }: Props) {
  const [items, setItems] = useState<TrashItem[] | null>(null)
  const [busyId, setBusyId] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [notice, setNotice] = useState<string | null>(null)

  const load = useCallback(() => {
    fetchTrash().then(setItems).catch((e) => setError(e instanceof Error ? e.message : String(e)))
  }, [])

  useEffect(() => { load() }, [load])

  async function act(id: string, fn: () => Promise<string | void>) {
    setBusyId(id)
    setError(null)
    setNotice(null)
    try {
      const r = await fn()
      if (typeof r === 'string') setNotice(`Restaurado en ${r}`)
      load()
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    } finally {
      setBusyId(null)
    }
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4" style={{ backgroundColor: 'rgba(0,0,0,0.5)' }} onClick={onClose}>
      <div
        className="w-full max-w-3xl max-h-[80vh] flex flex-col rounded-xl"
        style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center justify-between p-4" style={{ borderBottom: '1px solid var(--border)' }}>
          <div className="flex items-center gap-2">
            <Trash2 size={18} style={{ color: 'var(--accent)' }} />
            <h2 className="font-semibold" style={{ color: 'var(--text-primary)' }}>Papelera</h2>
            <span className="text-xs" style={{ color: 'var(--text-secondary)' }}>Se vacia sola a los 30 dias</span>
          </div>
          <div className="flex items-center gap-2">
            {isAdmin && items && items.length > 0 && (
              <button
                onClick={() => { if (confirm('Vaciar la papelera? Se borra todo definitivamente.')) act('all', () => emptyTrash().then(() => undefined)) }}
                className="px-3 py-1.5 rounded-lg text-xs"
                style={{ color: 'var(--danger)', border: '1px solid var(--danger)' }}
              >
                Vaciar
              </button>
            )}
            <button onClick={onClose} style={{ color: 'var(--text-secondary)' }}><X size={18} /></button>
          </div>
        </div>

        {(error || notice) && (
          <p className="px-4 pt-3 text-sm break-all" style={{ color: error ? 'var(--danger)' : 'var(--success)' }}>{error || notice}</p>
        )}

        <div className="overflow-auto p-4 space-y-2">
          {items === null && <Loader2 size={18} className="animate-spin" style={{ color: 'var(--text-secondary)' }} />}
          {items?.length === 0 && <p className="text-sm" style={{ color: 'var(--text-secondary)' }}>La papelera esta vacia.</p>}
          {items?.map((it) => (
            <div key={it.id} className="flex items-center gap-3 px-3 py-2 rounded-lg" style={{ backgroundColor: 'var(--bg-tertiary)' }}>
              {it.is_dir ? <Folder size={16} style={{ color: 'var(--accent)' }} /> : <File size={16} style={{ color: 'var(--text-secondary)' }} />}
              <div className="min-w-0 flex-1">
                <p className="text-sm truncate" style={{ color: 'var(--text-primary)' }}>{it.name}</p>
                <p className="text-xs truncate font-mono" style={{ color: 'var(--text-secondary)' }}>{it.original_path}</p>
                <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>
                  {formatSize(it.size)} · {it.deleted_by} · {new Date(it.deleted_at).toLocaleString()}
                </p>
              </div>
              {busyId === it.id ? <Loader2 size={16} className="animate-spin" /> : (
                <>
                  <button title="Restaurar" onClick={() => act(it.id, async () => { const p = await restoreTrashItem(it.id); onRestored(); return p })}
                    className="p-2 rounded-lg" style={{ color: 'var(--success)' }}><RotateCcw size={15} /></button>
                  <button title="Borrar definitivamente" onClick={() => { if (confirm(`Borrar "${it.name}" definitivamente?`)) act(it.id, () => deleteTrashItem(it.id)) }}
                    className="p-2 rounded-lg" style={{ color: 'var(--danger)' }}><Trash2 size={15} /></button>
                </>
              )}
            </div>
          ))}
        </div>
      </div>
    </div>
  )
}

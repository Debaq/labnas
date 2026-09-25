import { useEffect, useState } from 'react'
import { FolderLock, Plus, Trash2, Loader2, RotateCcw } from 'lucide-react'
import { fetchStorageRoots, saveStorageRoots } from '../api'

/** Admin: carpetas del servidor accesibles desde el explorador, compartir, descargar URL e imprimir */
export default function StorageRootsSection() {
  const [roots, setRoots] = useState<string[]>([])
  const [defaults, setDefaults] = useState<string[]>([])
  const [newRoot, setNewRoot] = useState('')
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [saved, setSaved] = useState(false)

  useEffect(() => {
    fetchStorageRoots()
      .then((r) => { setRoots(r.roots); setDefaults(r.defaults) })
      .catch((e) => setError(e.message))
  }, [])

  async function save(next: string[]) {
    setSaving(true)
    setError(null)
    setSaved(false)
    try {
      const r = await saveStorageRoots(next)
      setRoots(r.roots)
      setSaved(true)
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    } finally {
      setSaving(false)
    }
  }

  function add() {
    const value = newRoot.trim()
    if (!value || roots.includes(value)) return
    save([...roots, value]).then(() => setNewRoot(''))
  }

  return (
    <section>
      <div className="flex items-center gap-3 mb-4">
        <FolderLock size={22} style={{ color: 'var(--accent)' }} />
        <h2 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>
          Carpetas accesibles
        </h2>
      </div>
      <div
        className="rounded-xl p-6 space-y-4"
        style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}
      >
        <p className="text-sm" style={{ color: 'var(--text-secondary)' }}>
          Solo estas carpetas (y su contenido) se pueden ver, descargar, compartir o modificar desde LabNAS.
          El directorio de datos de LabNAS nunca es accesible.
        </p>

        <div className="space-y-2">
          {roots.map((r) => (
            <div
              key={r}
              className="flex items-center justify-between px-3 py-2 rounded-lg"
              style={{ backgroundColor: 'var(--bg-tertiary)' }}
            >
              <span className="text-sm font-mono" style={{ color: 'var(--text-primary)' }}>{r}</span>
              <button
                onClick={() => save(roots.filter((x) => x !== r))}
                disabled={saving || roots.length <= 1}
                title={roots.length <= 1 ? 'Debe quedar al menos una carpeta' : 'Quitar'}
                className="p-1 rounded disabled:opacity-30"
                style={{ color: 'var(--danger)' }}
              >
                <Trash2 size={15} />
              </button>
            </div>
          ))}
        </div>

        <div className="flex gap-2">
          <input
            value={newRoot}
            onChange={(e) => setNewRoot(e.target.value)}
            onKeyDown={(e) => { if (e.key === 'Enter') add() }}
            placeholder="/srv/datos"
            className="flex-1 px-3 py-2 rounded-lg text-sm font-mono outline-none"
            style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
          />
          <button
            onClick={add}
            disabled={saving || !newRoot.trim()}
            className="flex items-center gap-1 px-3 py-2 rounded-lg text-sm disabled:opacity-50"
            style={{ backgroundColor: 'var(--accent)', color: 'var(--bg-primary)' }}
          >
            {saving ? <Loader2 size={15} className="animate-spin" /> : <Plus size={15} />}
            Agregar
          </button>
          <button
            onClick={() => save(defaults)}
            disabled={saving}
            title={`Restaurar: ${defaults.join(', ')}`}
            className="flex items-center gap-1 px-3 py-2 rounded-lg text-sm disabled:opacity-50"
            style={{ backgroundColor: 'var(--bg-tertiary)', color: 'var(--text-secondary)' }}
          >
            <RotateCcw size={15} />
          </button>
        </div>

        {error && <p className="text-sm" style={{ color: 'var(--danger)' }}>{error}</p>}
        {saved && !error && <p className="text-sm" style={{ color: 'var(--success)' }}>Guardado</p>}
      </div>
    </section>
  )
}

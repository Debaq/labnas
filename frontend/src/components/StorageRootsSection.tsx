import { useEffect, useState } from 'react'
import { FolderLock, Plus, Trash2, Loader2, RotateCcw, X, HardDriveUpload } from 'lucide-react'
import { fetchStorageRoots, saveStorageRoots, fetchWebUsers, fetchWebdavSettings, saveWebdavSettings, type RootConfig } from '../api'
import { errorMessage } from '../lib/errors'

const DEFAULT_READERS = ['*']
const DEFAULT_WRITERS = ['perm:write']

function principalLabel(p: string): string {
  if (p === '*') return 'Todos'
  if (p === 'perm:write') return 'Con permiso de escritura'
  if (p.startsWith('role:')) return `Rol ${p.slice(5)}`
  if (p.startsWith('user:')) return p.slice(5)
  return p
}

const inputStyle = { backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }

function PrincipalList({ label, values, options, onChange, disabled }: {
  label: string
  values: string[]
  options: string[]
  onChange: (next: string[]) => void
  disabled: boolean
}) {
  const available = options.filter((o) => !values.includes(o))
  return (
    <div className="flex items-center gap-2 flex-wrap">
      <span className="text-xs w-16 shrink-0" style={{ color: 'var(--text-secondary)' }}>{label}</span>
      {values.length === 0 && <span className="text-xs italic" style={{ color: 'var(--text-secondary)' }}>solo admin</span>}
      {values.map((v) => (
        <span key={v} className="flex items-center gap-1 text-xs px-2 py-0.5 rounded-full" style={{ backgroundColor: 'var(--bg-secondary)', color: 'var(--text-primary)' }}>
          {principalLabel(v)}
          <button disabled={disabled} onClick={() => onChange(values.filter((x) => x !== v))} style={{ color: 'var(--text-secondary)' }}><X size={11} /></button>
        </span>
      ))}
      {available.length > 0 && (
        <select
          value=""
          disabled={disabled}
          onChange={(e) => { if (e.target.value) onChange([...values, e.target.value]) }}
          className="text-xs px-2 py-0.5 rounded-lg outline-none"
          style={inputStyle}
        >
          <option value="">+ agregar</option>
          {available.map((o) => <option key={o} value={o}>{principalLabel(o)}</option>)}
        </select>
      )}
    </div>
  )
}

/** Admin: carpetas del servidor accesibles desde LabNAS y quien lee/escribe en cada una */
export default function StorageRootsSection() {
  const [roots, setRoots] = useState<RootConfig[]>([])
  const [defaults, setDefaults] = useState<string[]>([])
  const [users, setUsers] = useState<string[]>([])
  const [newRoot, setNewRoot] = useState('')
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [saved, setSaved] = useState(false)
  const [webdav, setWebdav] = useState<boolean | null>(null)

  useEffect(() => {
    fetchWebdavSettings().then((w) => setWebdav(w.enabled)).catch(() => {})
    fetchStorageRoots()
      .then((r) => { setRoots(r.roots); setDefaults(r.defaults) })
      .catch((e) => setError(errorMessage(e)))
    fetchWebUsers()
      .then((u) => setUsers(u.filter((x) => x.role !== 'admin' && x.role !== 'pendiente').map((x) => x.username)))
      .catch(() => {})
  }, [])

  const options = ['*', 'perm:write', 'role:operador', 'role:observador', ...users.map((u) => `user:${u}`)]

  async function save(next: RootConfig[]) {
    setSaving(true)
    setError(null)
    setSaved(false)
    try {
      const r = await saveStorageRoots(next)
      setRoots(r.roots)
      setSaved(true)
    } catch (e) {
      setError(errorMessage(e))
    } finally {
      setSaving(false)
    }
  }

  function add() {
    const value = newRoot.trim()
    if (!value || roots.some((r) => r.path === value)) return
    save([...roots, { path: value, readers: DEFAULT_READERS, writers: DEFAULT_WRITERS }]).then(() => setNewRoot(''))
  }

  function update(path: string, patch: Partial<RootConfig>) {
    save(roots.map((r) => (r.path === path ? { ...r, ...patch } : r)))
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
          En cada una se elige quien lee y quien escribe (escribir incluye leer). El admin siempre tiene acceso.
          El directorio de datos de LabNAS nunca es accesible.
        </p>

        <div className="space-y-3">
          {roots.map((r) => (
            <div key={r.path} className="px-3 py-3 rounded-lg space-y-2" style={{ backgroundColor: 'var(--bg-tertiary)' }}>
              <div className="flex items-center justify-between">
                <span className="text-sm font-mono" style={{ color: 'var(--text-primary)' }}>{r.path}</span>
                <button
                  onClick={() => save(roots.filter((x) => x.path !== r.path))}
                  disabled={saving || roots.length <= 1}
                  title={roots.length <= 1 ? 'Debe quedar al menos una carpeta' : 'Quitar'}
                  className="p-1 rounded disabled:opacity-30"
                  style={{ color: 'var(--danger)' }}
                >
                  <Trash2 size={15} />
                </button>
              </div>
              <PrincipalList label="Leen" values={r.readers} options={options} disabled={saving} onChange={(readers) => update(r.path, { readers })} />
              <PrincipalList label="Escriben" values={r.writers} options={options} disabled={saving} onChange={(writers) => update(r.path, { writers })} />
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
            style={inputStyle}
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
            onClick={() => save(defaults.map((path) => ({ path, readers: DEFAULT_READERS, writers: DEFAULT_WRITERS })))}
            disabled={saving}
            title={`Restaurar: ${defaults.join(', ')} (con permisos por defecto)`}
            className="flex items-center gap-1 px-3 py-2 rounded-lg text-sm disabled:opacity-50"
            style={{ backgroundColor: 'var(--bg-tertiary)', color: 'var(--text-secondary)' }}
          >
            <RotateCcw size={15} />
          </button>
        </div>

        {error && <p className="text-sm" style={{ color: 'var(--danger)' }}>{error}</p>}
        {saved && !error && <p className="text-sm" style={{ color: 'var(--success)' }}>Guardado</p>}

        <div className="pt-4 space-y-2" style={{ borderTop: '1px solid var(--border)' }}>
          <label className="flex items-center gap-2 text-sm font-medium" style={{ color: 'var(--text-primary)' }}>
            <input
              type="checkbox"
              checked={webdav ?? false}
              disabled={webdav === null}
              onChange={(e) => {
                const v = e.target.checked
                saveWebdavSettings(v).then((w) => setWebdav(w.enabled)).catch((err) => setError(errorMessage(err)))
              }}
            />
            <HardDriveUpload size={15} style={{ color: 'var(--accent)' }} />
            Montar como unidad de red (WebDAV)
          </label>
          {webdav && (
            <div className="text-xs space-y-1" style={{ color: 'var(--text-secondary)' }}>
              <p>
                Direccion: <code style={{ color: 'var(--text-primary)' }}>{`${window.location.protocol}//${window.location.host}/dav/`}</code> con el mismo usuario y contraseña de la web.
                Se aplican los mismos permisos por carpeta; borrar envia a la papelera.
              </p>
              <p>Linux: Archivos &gt; Otras ubicaciones &gt; <code>dav://{window.location.host}/dav/</code> · macOS: Finder &gt; Ir &gt; Conectarse al servidor.</p>
              <p>Windows sin HTTPS: habilitar Basic sobre HTTP (registro WebClient <code>BasicAuthLevel=2</code>) o usar un cliente como WinSCP/Cyberduck.</p>
            </div>
          )}
        </div>
      </div>
    </section>
  )
}

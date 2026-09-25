import { useCallback, useEffect, useState } from 'react'
import { Archive, Plus, Play, Pencil, Trash2, Loader2, AlertTriangle, CheckCircle2, XCircle, Database, X } from 'lucide-react'
import { fetchBackups, saveBackup, deleteBackup, runBackup, fetchBackupSnapshots, type BackupJob, type BackupJobInput, type BackupsInfo } from '../api'

const EMPTY: BackupJobInput = { name: '', source: '', destination: '', hour: 3, minute: 0, keep: 7, include_db: true, enabled: true }

const card = { backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }
const input = { backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }

function errMsg(e: unknown): string {
  return e instanceof Error ? e.message : String(e)
}

function StatusBadge({ job }: { job: BackupJob }) {
  if (job.running || job.last_status === 'running') {
    return <span className="flex items-center gap-1 text-xs" style={{ color: 'var(--accent)' }}><Loader2 size={13} className="animate-spin" />Ejecutando</span>
  }
  if (!job.last_status) return <span className="text-xs" style={{ color: 'var(--text-secondary)' }}>Nunca ejecutado</span>
  const map = {
    ok: { icon: CheckCircle2, color: 'var(--success)', label: 'OK' },
    warning: { icon: AlertTriangle, color: 'var(--warning)', label: 'Con advertencias' },
    error: { icon: XCircle, color: 'var(--danger)', label: 'Error' },
  } as const
  const s = map[job.last_status as keyof typeof map]
  if (!s) return null
  const Icon = s.icon
  return <span className="flex items-center gap-1 text-xs" style={{ color: s.color }}><Icon size={13} />{s.label}</span>
}

/** Admin: tareas de respaldo con snapshots incrementales (rsync) */
export default function BackupsSection() {
  const [info, setInfo] = useState<BackupsInfo | null>(null)
  const [editing, setEditing] = useState<{ id?: string; job: BackupJobInput } | null>(null)
  const [snapshots, setSnapshots] = useState<{ job: BackupJob; list: string[] } | null>(null)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const load = useCallback(() => {
    fetchBackups().then(setInfo).catch((e) => setError(errMsg(e)))
  }, [])

  useEffect(() => { load() }, [load])

  // Refrescar mientras haya alguno ejecutandose
  const anyRunning = info?.jobs.some((j) => j.running || j.last_status === 'running') ?? false
  useEffect(() => {
    if (!anyRunning) return
    const t = setInterval(load, 3000)
    return () => clearInterval(t)
  }, [anyRunning, load])

  async function submit() {
    if (!editing) return
    setBusy(true)
    setError(null)
    try {
      await saveBackup(editing.job, editing.id)
      setEditing(null)
      load()
    } catch (e) {
      setError(errMsg(e))
    } finally {
      setBusy(false)
    }
  }

  async function act(fn: () => Promise<void>) {
    setError(null)
    try { await fn(); load() } catch (e) { setError(errMsg(e)) }
  }

  const set = <K extends keyof BackupJobInput>(k: K, v: BackupJobInput[K]) =>
    setEditing((cur) => (cur ? { ...cur, job: { ...cur.job, [k]: v } } : cur))

  return (
    <section className="space-y-4">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-3">
          <Archive size={22} style={{ color: 'var(--accent)' }} />
          <h2 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>Respaldos</h2>
        </div>
        {!editing && (
          <button
            onClick={() => setEditing({ job: { ...EMPTY } })}
            className="flex items-center gap-1 px-3 py-2 rounded-lg text-sm"
            style={{ backgroundColor: 'var(--accent)', color: 'var(--bg-primary)' }}
          >
            <Plus size={15} /> Nuevo respaldo
          </button>
        )}
      </div>

      {info && !info.rsync_available && (
        <div className="rounded-xl p-4 flex items-center gap-2 text-sm" style={{ ...card, color: 'var(--warning)' }}>
          <AlertTriangle size={16} /> rsync no esta instalado en el servidor. Instalalo (ej. <code>sudo pacman -S rsync</code>) para usar respaldos.
        </div>
      )}

      {error && <p className="text-sm" style={{ color: 'var(--danger)' }}>{error}</p>}

      {editing && (
        <div className="rounded-xl p-6 space-y-3" style={card}>
          <h3 className="text-sm font-semibold" style={{ color: 'var(--text-primary)' }}>
            {editing.id ? 'Editar respaldo' : 'Nuevo respaldo'}
          </h3>
          <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
            <label className="text-xs space-y-1" style={{ color: 'var(--text-secondary)' }}>
              <span>Nombre</span>
              <input value={editing.job.name} onChange={(e) => set('name', e.target.value)} placeholder="Documentos del lab"
                className="w-full px-3 py-2 rounded-lg text-sm outline-none" style={input} />
            </label>
            <label className="text-xs space-y-1" style={{ color: 'var(--text-secondary)' }}>
              <span>Hora diaria</span>
              <input type="time" value={`${String(editing.job.hour).padStart(2, '0')}:${String(editing.job.minute).padStart(2, '0')}`}
                onChange={(e) => { const [h, m] = e.target.value.split(':').map(Number); set('hour', h || 0); set('minute', m || 0) }}
                className="w-full px-3 py-2 rounded-lg text-sm outline-none" style={input} />
            </label>
            <label className="text-xs space-y-1" style={{ color: 'var(--text-secondary)' }}>
              <span>Origen (carpeta dentro de las carpetas accesibles)</span>
              <input value={editing.job.source} onChange={(e) => set('source', e.target.value)} placeholder="/home/lab/Documentos"
                className="w-full px-3 py-2 rounded-lg text-sm font-mono outline-none" style={input} />
            </label>
            <label className="text-xs space-y-1" style={{ color: 'var(--text-secondary)' }}>
              <span>Destino (disco externo o carpeta de red montada)</span>
              <input value={editing.job.destination} onChange={(e) => set('destination', e.target.value)} placeholder="/mnt/respaldo/documentos"
                className="w-full px-3 py-2 rounded-lg text-sm font-mono outline-none" style={input} />
            </label>
            <label className="text-xs space-y-1" style={{ color: 'var(--text-secondary)' }}>
              <span>Copias a conservar</span>
              <input type="number" min={1} max={365} value={editing.job.keep} onChange={(e) => set('keep', parseInt(e.target.value) || 1)}
                className="w-full px-3 py-2 rounded-lg text-sm outline-none" style={input} />
            </label>
            <div className="flex flex-col justify-end gap-2 text-sm" style={{ color: 'var(--text-primary)' }}>
              <label className="flex items-center gap-2">
                <input type="checkbox" checked={editing.job.include_db} onChange={(e) => set('include_db', e.target.checked)} />
                Incluir la base de datos de LabNAS
              </label>
              <label className="flex items-center gap-2">
                <input type="checkbox" checked={editing.job.enabled} onChange={(e) => set('enabled', e.target.checked)} />
                Activo
              </label>
            </div>
          </div>
          <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>
            Cada copia es completa y navegable, pero los archivos sin cambios se comparten con la copia anterior (no ocupan espacio de nuevo).
          </p>
          <div className="flex gap-2 justify-end">
            <button onClick={() => setEditing(null)} className="px-3 py-2 rounded-lg text-sm" style={{ color: 'var(--text-secondary)', border: '1px solid var(--border)' }}>
              Cancelar
            </button>
            <button onClick={submit} disabled={busy} className="flex items-center gap-1 px-3 py-2 rounded-lg text-sm disabled:opacity-50"
              style={{ backgroundColor: 'var(--accent)', color: 'var(--bg-primary)' }}>
              {busy && <Loader2 size={15} className="animate-spin" />} Guardar
            </button>
          </div>
        </div>
      )}

      {info?.jobs.map((job) => (
        <div key={job.id} className="rounded-xl p-5 space-y-2" style={card}>
          <div className="flex items-center justify-between gap-3">
            <div className="min-w-0">
              <div className="flex items-center gap-2">
                <span className="text-sm font-semibold" style={{ color: 'var(--text-primary)' }}>{job.name}</span>
                {!job.enabled && <span className="text-xs px-2 rounded" style={{ backgroundColor: 'var(--bg-tertiary)', color: 'var(--text-secondary)' }}>Pausado</span>}
              </div>
              <p className="text-xs font-mono truncate" style={{ color: 'var(--text-secondary)' }}>{job.source} → {job.destination}</p>
            </div>
            <div className="flex items-center gap-1 shrink-0">
              <button title="Ejecutar ahora" disabled={job.running} onClick={() => act(() => runBackup(job.id))}
                className="p-2 rounded-lg disabled:opacity-30" style={{ color: 'var(--accent)' }}><Play size={15} /></button>
              <button title="Ver copias" onClick={() => fetchBackupSnapshots(job.id).then((list) => setSnapshots({ job, list })).catch((e) => setError(errMsg(e)))}
                className="p-2 rounded-lg" style={{ color: 'var(--text-secondary)' }}><Archive size={15} /></button>
              <button title="Editar" onClick={() => setEditing({ id: job.id, job: { name: job.name, source: job.source, destination: job.destination, hour: job.hour, minute: job.minute, keep: job.keep, include_db: job.include_db, enabled: job.enabled } })}
                className="p-2 rounded-lg" style={{ color: 'var(--text-secondary)' }}><Pencil size={15} /></button>
              <button title="Eliminar tarea (no borra las copias)" onClick={() => { if (confirm(`Eliminar la tarea "${job.name}"? Las copias ya hechas no se borran.`)) act(() => deleteBackup(job.id)) }}
                className="p-2 rounded-lg" style={{ color: 'var(--danger)' }}><Trash2 size={15} /></button>
            </div>
          </div>
          <div className="flex flex-wrap items-center gap-x-4 gap-y-1 text-xs" style={{ color: 'var(--text-secondary)' }}>
            <span>Diario {String(job.hour).padStart(2, '0')}:{String(job.minute).padStart(2, '0')}</span>
            <span>Conserva {job.keep}</span>
            {job.include_db && <span className="flex items-center gap-1"><Database size={12} />Incluye DB</span>}
            <StatusBadge job={job} />
            {job.last_run && <span>Ultima: {new Date(job.last_run).toLocaleString()}</span>}
          </div>
          {job.last_message && job.last_status !== 'ok' && (
            <p className="text-xs break-all" style={{ color: job.last_status === 'error' ? 'var(--danger)' : 'var(--warning)' }}>{job.last_message}</p>
          )}
        </div>
      ))}

      {info && info.jobs.length === 0 && !editing && (
        <div className="rounded-xl p-6 text-sm" style={{ ...card, color: 'var(--text-secondary)' }}>
          No hay respaldos configurados.
        </div>
      )}

      {snapshots && (
        <div className="rounded-xl p-5 space-y-2" style={card}>
          <div className="flex items-center justify-between">
            <span className="text-sm font-semibold" style={{ color: 'var(--text-primary)' }}>Copias de "{snapshots.job.name}"</span>
            <button onClick={() => setSnapshots(null)} style={{ color: 'var(--text-secondary)' }}><X size={16} /></button>
          </div>
          {snapshots.list.length === 0
            ? <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>Todavia no hay copias.</p>
            : snapshots.list.map((s) => (
              <p key={s} className="text-xs font-mono" style={{ color: 'var(--text-primary)' }}>{snapshots.job.destination}/{s}</p>
            ))}
        </div>
      )}

      {info && (
        <div className="rounded-xl p-5 space-y-1" style={card}>
          <div className="flex items-center gap-2 text-sm font-semibold" style={{ color: 'var(--text-primary)' }}>
            <Database size={15} /> Copia automatica de la base de datos
          </div>
          <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>
            Todos los dias en <code>{info.db_backups_dir}</code> (se conservan 7).
            {info.db_backups.length > 0 ? ` Ultima: ${info.db_backups[0]}` : ' Aun no hay copias.'}
          </p>
        </div>
      )}
    </section>
  )
}

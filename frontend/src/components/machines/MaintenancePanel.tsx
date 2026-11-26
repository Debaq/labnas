import { useCallback, useEffect, useState } from 'react'
import { Camera, Check, Gauge, Loader2, Lock, Pencil, Plus, Trash2, Wrench, X } from 'lucide-react'
import {
  addMaintenanceLog, compressImage, createMaintenanceTask, deleteMaintenanceTask, fetchMaintenance,
  machinePhotoUrl, maintenanceDue, maintenanceEvery, MAINTENANCE_LEVEL, setMachineUsage, shortDateTime,
  updateMaintenanceTask,
  type MachineDetail, type MaintenanceOverview, type MaintenanceTask, type MaintenanceTaskInput,
} from '../../api'
import { useToast } from '../ToastContext'

const inputStyle = { backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }
const card = { backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }
const btn = { backgroundColor: 'var(--bg-tertiary)', color: 'var(--text-primary)' }

const LOG_KINDS: Record<string, string> = {
  mantencion: 'Mantencion',
  calibracion: 'Calibracion',
  reparacion: 'Reparacion',
  nota: 'Nota',
}

const EMPTY: MaintenanceTaskInput = { title: '', kind: 'mantencion', every_days: null, every_hours: null, instructions: '', blocks: false }

function TaskForm({ initial, onSave, onCancel }: { initial: MaintenanceTaskInput; onSave: (t: MaintenanceTaskInput) => Promise<boolean>; onCancel: () => void }) {
  const [t, setT] = useState(initial)
  const num = (v: string) => (v.trim() === '' ? null : Number(v.replace(',', '.')))
  return (
    <div className="rounded-lg p-3 space-y-2" style={{ border: '1px solid var(--border)' }}>
      <div className="grid grid-cols-1 sm:grid-cols-3 gap-2">
        <input value={t.title} onChange={e => setT({ ...t, title: e.target.value })} placeholder="Tarea: limpiar lente, calibrar cama…" className="sm:col-span-2 rounded-lg px-2 py-1.5 text-sm" style={inputStyle} />
        <select value={t.kind} onChange={e => setT({ ...t, kind: e.target.value as MaintenanceTaskInput['kind'] })} className="rounded-lg px-2 py-1.5 text-sm" style={inputStyle}>
          <option value="mantencion">Mantencion</option>
          <option value="calibracion">Calibracion</option>
        </select>
      </div>
      <div className="grid grid-cols-2 gap-2">
        <label className="text-xs space-y-1" style={{ color: 'var(--text-secondary)' }}>
          <span>Cada cuantos dias</span>
          <input type="number" min={1} value={t.every_days ?? ''} onChange={e => setT({ ...t, every_days: num(e.target.value) })} placeholder="—" className="w-full rounded-lg px-2 py-1.5 text-sm" style={inputStyle} />
        </label>
        <label className="text-xs space-y-1" style={{ color: 'var(--text-secondary)' }}>
          <span>Cada cuantas horas de uso</span>
          <input type="number" min={0.5} step={0.5} value={t.every_hours ?? ''} onChange={e => setT({ ...t, every_hours: num(e.target.value) })} placeholder="—" className="w-full rounded-lg px-2 py-1.5 text-sm" style={inputStyle} />
        </label>
      </div>
      <textarea value={t.instructions} onChange={e => setT({ ...t, instructions: e.target.value })} rows={2} placeholder="Como se hace (opcional)" className="w-full rounded-lg px-2 py-1.5 text-sm" style={inputStyle} />
      <label className="flex items-center gap-2 text-xs" style={{ color: 'var(--text-secondary)' }}>
        <input type="checkbox" checked={t.blocks} onChange={e => setT({ ...t, blocks: e.target.checked })} />
        Bloquear las reservas mientras este vencida (revisiones de seguridad)
      </label>
      <div className="flex justify-end gap-2">
        <button onClick={onCancel} className="px-3 py-1.5 rounded-lg text-sm" style={btn}>Cancelar</button>
        <button onClick={() => onSave(t)} className="px-3 py-1.5 rounded-lg text-sm" style={{ backgroundColor: 'var(--accent)', color: '#fff' }}>Guardar</button>
      </div>
    </div>
  )
}

/** Anotar en la bitacora: una tarea del plan o un registro suelto */
function LogForm({ task, onSave, onCancel }: { task?: MaintenanceTask; onSave: (d: { kind?: string; title?: string; note: string; photo?: string }) => Promise<boolean>; onCancel: () => void }) {
  const [kind, setKind] = useState('mantencion')
  const [title, setTitle] = useState('')
  const [note, setNote] = useState('')
  const [photo, setPhoto] = useState<string | undefined>()
  return (
    <div className="rounded-lg p-3 space-y-2" style={{ border: '1px solid var(--border)' }}>
      {task ? (
        <p className="text-sm" style={{ color: 'var(--text-primary)' }}>Registrar «{task.title}» como hecha ahora</p>
      ) : (
        <div className="grid grid-cols-1 sm:grid-cols-3 gap-2">
          <select value={kind} onChange={e => setKind(e.target.value)} className="rounded-lg px-2 py-1.5 text-sm" style={inputStyle}>
            {Object.entries(LOG_KINDS).map(([k, l]) => <option key={k} value={k}>{l}</option>)}
          </select>
          <input value={title} onChange={e => setTitle(e.target.value)} placeholder="Que se hizo" className="sm:col-span-2 rounded-lg px-2 py-1.5 text-sm" style={inputStyle} />
        </div>
      )}
      <textarea value={note} onChange={e => setNote(e.target.value)} rows={2} placeholder="Detalle, repuestos, valores medidos (opcional)" className="w-full rounded-lg px-2 py-1.5 text-sm" style={inputStyle} />
      <div className="flex items-center justify-between gap-2">
        <label className="flex items-center gap-1 text-xs cursor-pointer" style={{ color: photo ? 'var(--success)' : 'var(--text-secondary)' }}>
          <Camera size={14} /> {photo ? 'Foto lista' : 'Foto (opcional)'}
          <input type="file" accept="image/*" capture="environment" className="hidden" onChange={async e => { const f = e.target.files?.[0]; if (f) setPhoto(await compressImage(f)) }} />
        </label>
        <div className="flex gap-2">
          <button onClick={onCancel} className="px-3 py-1.5 rounded-lg text-sm" style={btn}>Cancelar</button>
          <button onClick={() => onSave(task ? { note, photo } : { kind, title, note, photo })} className="px-3 py-1.5 rounded-lg text-sm" style={{ backgroundColor: 'var(--accent)', color: '#fff' }}>Registrar</button>
        </div>
      </div>
    </div>
  )
}

/** Plan de mantencion y calibracion, horas de uso y bitacora de la maquina */
export default function MaintenancePanel({ machine, me, staff }: { machine: MachineDetail; me: string; staff: boolean }) {
  const { addToast } = useToast()
  const [data, setData] = useState<MaintenanceOverview | null>(null)
  const [editing, setEditing] = useState<MaintenanceTask | 'new' | null>(null)
  const [logging, setLogging] = useState<MaintenanceTask | 'free' | null>(null)
  const [usageInput, setUsageInput] = useState<string | null>(null)
  const canLog = staff || machine.responsible === me
  const id = machine.id

  const load = useCallback(() => { fetchMaintenance(id).then(setData).catch(() => {}) }, [id])
  useEffect(() => { load() }, [load, machine])

  async function run<T>(fn: () => Promise<T>, ok: string, after?: (r: T) => void) {
    try {
      const r = await fn()
      addToast(ok, 'success')
      after?.(r)
      return true
    } catch (e) {
      addToast((e as Error).message, 'error')
      return false
    }
  }

  if (!data) return <div className="rounded-xl p-4" style={card}><Loader2 size={18} className="animate-spin" style={{ color: 'var(--accent)' }} /></div>
  if (!staff && !canLog && data.tasks.length === 0 && data.log.length === 0) return null

  return (
    <div className="rounded-xl p-4 space-y-3" style={card}>
      <div className="flex items-center justify-between gap-2 flex-wrap">
        <h2 className="text-sm font-semibold flex items-center gap-2" style={{ color: 'var(--text-primary)' }}>
          <Wrench size={16} /> Mantencion y calibracion
        </h2>
        <span className="text-xs flex items-center gap-1" style={{ color: 'var(--text-secondary)' }} title={`${data.booked_hours} h de reservas terminadas${data.adjust_hours ? `, ajuste ${data.adjust_hours > 0 ? '+' : ''}${data.adjust_hours} h por horometro` : ''}`}>
          <Gauge size={14} /> {data.usage_hours} h de uso
          {staff && usageInput === null && (
            <button onClick={() => setUsageInput(String(data.usage_hours))} className="underline ml-1" style={{ color: 'var(--accent)' }}>fijar</button>
          )}
        </span>
      </div>

      {usageInput !== null && (
        <div className="flex items-center gap-2 text-xs" style={{ color: 'var(--text-secondary)' }}>
          Lectura del horometro:
          <input type="number" min={0} step={0.1} value={usageInput} onChange={e => setUsageInput(e.target.value)} className="w-28 rounded-lg px-2 py-1 text-sm" style={inputStyle} />
          <button onClick={() => run(() => setMachineUsage(id, Number(usageInput)), 'Horas de uso actualizadas', r => { setData(r); setUsageInput(null) })} className="px-2 py-1 rounded-lg" style={{ backgroundColor: 'var(--accent)', color: '#fff' }}>Guardar</button>
          <button onClick={() => setUsageInput(null)} aria-label="Cancelar"><X size={14} /></button>
        </div>
      )}

      {data.tasks.length === 0 && !editing && (
        <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>
          Sin plan de mantencion. {staff ? 'Agrega tareas que se repiten por fecha o por horas de uso: las de fecha salen en Plazos y las de horas avisan por Telegram.' : ''}
        </p>
      )}

      <div className="space-y-2">
        {data.tasks.map(t => editing && editing !== 'new' && editing.id === t.id ? (
          <TaskForm key={t.id} initial={t} onCancel={() => setEditing(null)} onSave={v => run(() => updateMaintenanceTask(t.id, v), 'Tarea guardada', () => { setEditing(null); load() })} />
        ) : (
          <div key={t.id} className="space-y-2">
            <div className="flex items-start justify-between gap-2 p-2 rounded-lg" style={{ borderLeft: `3px solid ${MAINTENANCE_LEVEL[t.level].color}`, backgroundColor: 'var(--bg-secondary)' }}>
              <div className="min-w-0">
                <div className="text-sm flex items-center gap-2 flex-wrap" style={{ color: 'var(--text-primary)' }}>
                  {t.title}
                  <span className="text-[11px] px-1.5 rounded" style={{ color: 'var(--text-secondary)', border: '1px solid var(--border)' }}>{t.kind === 'calibracion' ? 'Calibracion' : 'Mantencion'}</span>
                  {t.blocks && <span title="Bloquea las reservas mientras este vencida"><Lock size={12} style={{ color: 'var(--text-secondary)' }} /></span>}
                </div>
                <div className="text-xs" style={{ color: 'var(--text-secondary)' }}>
                  {maintenanceEvery(t)} · <span style={{ color: MAINTENANCE_LEVEL[t.level].color }}>{maintenanceDue(t)}</span>
                  {t.last_done_at && <> · ultima: {shortDateTime(t.last_done_at)}</>}
                </div>
                {t.instructions && <div className="text-xs mt-1 whitespace-pre-wrap" style={{ color: 'var(--text-secondary)' }}>{t.instructions}</div>}
              </div>
              <div className="flex items-center gap-1 shrink-0">
                {canLog && (
                  <button onClick={() => setLogging(t)} className="flex items-center gap-1 px-2 py-1 rounded-lg text-xs" style={{ backgroundColor: 'var(--accent)', color: '#fff' }}>
                    <Check size={13} /> Hecho
                  </button>
                )}
                {staff && <button onClick={() => setEditing(t)} className="p-1" title="Editar"><Pencil size={13} style={{ color: 'var(--text-secondary)' }} /></button>}
                {staff && (
                  <button onClick={() => confirm(`Quitar «${t.title}» del plan? La bitacora se conserva.`) && run(() => deleteMaintenanceTask(t.id), 'Tarea quitada', load)} className="p-1" title="Quitar">
                    <Trash2 size={13} style={{ color: 'var(--danger)' }} />
                  </button>
                )}
              </div>
            </div>
            {logging !== null && logging !== 'free' && logging.id === t.id && (
              <LogForm task={t} onCancel={() => setLogging(null)} onSave={d => run(() => addMaintenanceLog(id, { task_id: t.id, ...d }), 'Registrado en la bitacora', r => { setData(r); setLogging(null) })} />
            )}
          </div>
        ))}
      </div>

      {editing === 'new' && <TaskForm initial={EMPTY} onCancel={() => setEditing(null)} onSave={v => run(() => createMaintenanceTask(id, v), 'Tarea agregada', () => { setEditing(null); load() })} />}
      {logging === 'free' && <LogForm onCancel={() => setLogging(null)} onSave={d => run(() => addMaintenanceLog(id, d), 'Registrado en la bitacora', r => { setData(r); setLogging(null) })} />}

      {(staff || canLog) && !editing && !logging && (
        <div className="flex gap-2">
          {staff && <button onClick={() => setEditing('new')} className="flex items-center gap-1 px-3 py-1.5 rounded-lg text-xs" style={btn}><Plus size={13} /> Tarea del plan</button>}
          {canLog && <button onClick={() => setLogging('free')} className="flex items-center gap-1 px-3 py-1.5 rounded-lg text-xs" style={btn}><Pencil size={13} /> Anotar en la bitacora</button>}
        </div>
      )}

      {data.log.length > 0 && (
        <details>
          <summary className="text-sm cursor-pointer" style={{ color: 'var(--text-primary)' }}>Bitacora ({data.log.length})</summary>
          <ul className="mt-2 space-y-2">
            {data.log.map(l => (
              <li key={l.id} className="text-xs p-2 rounded-lg" style={{ backgroundColor: 'var(--bg-secondary)' }}>
                <div className="flex justify-between gap-2" style={{ color: 'var(--text-primary)' }}>
                  <span><b>{LOG_KINDS[l.kind] ?? l.kind}:</b> {l.title}</span>
                  <span className="shrink-0" style={{ color: 'var(--text-secondary)' }}>{shortDateTime(l.done_at)}</span>
                </div>
                <div style={{ color: 'var(--text-secondary)' }}>{l.done_by} · {l.usage_hours} h de uso</div>
                {l.note && <div className="whitespace-pre-wrap mt-1" style={{ color: 'var(--text-primary)' }}>{l.note}</div>}
                {l.photo_id && <a href={machinePhotoUrl(l.photo_id)} target="_blank" rel="noreferrer" className="underline" style={{ color: 'var(--accent)' }}>Ver foto</a>}
              </li>
            ))}
          </ul>
        </details>
      )}
    </div>
  )
}

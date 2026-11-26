import { useCallback, useEffect, useState } from 'react'
import { Camera, Pencil, Plus, SlidersHorizontal, ThumbsUp, Trash2 } from 'lucide-react'
import {
  compressImage, createCutParam, deleteCutParam, fetchCutParams, machinePhotoUrl, updateCutParam, voteCutParam,
  type CutParam, type CutParamInput,
} from '../../api'
import { useAuth } from '../../auth/AuthContext'
import { useToast } from '../ToastContext'

const card = { backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }
const input = { backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }
const RESULT_COLOR: Record<CutParam['result'], string> = { bueno: 'var(--success)', regular: 'var(--warning, #d97706)', malo: 'var(--danger)' }
const EMPTY: CutParamInput = { material: '', thickness_mm: null, operation: 'corte', power: null, speed: null, passes: null, notes: '', result: 'bueno' }

function ParamForm({ initial, units, onSave, onCancel }: { initial: CutParamInput; units: { power: string; speed: string }; onSave: (d: CutParamInput) => void; onCancel: () => void }) {
  const [f, setF] = useState(initial)
  const num = (v: string) => (v.trim() === '' ? null : Number(v.replace(',', '.')))
  return (
    <div className="rounded-lg p-3 space-y-2" style={{ border: '1px solid var(--border)' }}>
      <div className="grid grid-cols-2 sm:grid-cols-4 gap-2">
        <input value={f.material} onChange={e => setF({ ...f, material: e.target.value })} placeholder="Material: MDF, acrilico…" className="col-span-2 rounded-lg px-2 py-1.5 text-sm" style={input} />
        <input type="number" step="0.1" value={f.thickness_mm ?? ''} onChange={e => setF({ ...f, thickness_mm: num(e.target.value) })} placeholder="Espesor mm" className="rounded-lg px-2 py-1.5 text-sm" style={input} />
        <select value={f.operation} onChange={e => setF({ ...f, operation: e.target.value as CutParam['operation'] })} className="rounded-lg px-2 py-1.5 text-sm" style={input}>
          {['corte', 'grabado', 'marcado', 'fresado', 'otro'].map(o => <option key={o} value={o}>{o}</option>)}
        </select>
        <input type="number" step="any" value={f.power ?? ''} onChange={e => setF({ ...f, power: num(e.target.value) })} placeholder={`Potencia ${units.power}`} className="rounded-lg px-2 py-1.5 text-sm" style={input} />
        <input type="number" step="any" value={f.speed ?? ''} onChange={e => setF({ ...f, speed: num(e.target.value) })} placeholder={`Velocidad ${units.speed}`} className="rounded-lg px-2 py-1.5 text-sm" style={input} />
        <input type="number" min={1} value={f.passes ?? ''} onChange={e => setF({ ...f, passes: num(e.target.value) })} placeholder="Pasadas" className="rounded-lg px-2 py-1.5 text-sm" style={input} />
        <select value={f.result} onChange={e => setF({ ...f, result: e.target.value as CutParam['result'] })} className="rounded-lg px-2 py-1.5 text-sm" style={input}>
          <option value="bueno">Resultado bueno</option>
          <option value="regular">Regular</option>
          <option value="malo">Malo (no usar)</option>
        </select>
      </div>
      <textarea value={f.notes} onChange={e => setF({ ...f, notes: e.target.value })} rows={2} placeholder="Notas: foco, aire, orden de cortes…" className="w-full rounded-lg px-2 py-1.5 text-sm" style={input} />
      <div className="flex items-center justify-between gap-2">
        <label className="flex items-center gap-1 text-xs cursor-pointer" style={{ color: f.photo ? 'var(--success)' : 'var(--text-secondary)' }}>
          <Camera size={14} /> {f.photo ? 'Foto lista' : 'Foto del resultado (opcional)'}
          <input type="file" accept="image/*" capture="environment" className="hidden" onChange={async e => { const file = e.target.files?.[0]; if (file) setF({ ...f, photo: await compressImage(file) }) }} />
        </label>
        <div className="flex gap-2">
          <button onClick={onCancel} className="px-3 py-1.5 rounded-lg text-sm" style={{ color: 'var(--text-secondary)' }}>Cancelar</button>
          <button onClick={() => onSave(f)} className="px-3 py-1.5 rounded-lg text-sm" style={{ backgroundColor: 'var(--accent)', color: '#fff' }}>Guardar</button>
        </div>
      </div>
    </div>
  )
}

/** Parametros de corte de una maquina (o de todas si no se pasa `machineId`) */
export default function CutParamsPanel({ machineId, kind }: { machineId?: string; kind?: string }) {
  const { user, canModule } = useAuth()
  const { addToast } = useToast()
  const staff = canModule('machines', 'gestionar')
  const canAdd = !!machineId && canModule('machines', 'usar')
  const [list, setList] = useState<CutParam[]>([])
  const [q, setQ] = useState('')
  const [editing, setEditing] = useState<CutParam | 'new' | null>(null)
  const units = kind === 'cnc' ? { power: '(RPM)', speed: '(mm/min)' } : { power: '(%)', speed: '(mm/s)' }

  const load = useCallback(() => { fetchCutParams({ machine: machineId, q }).then(setList).catch(() => {}) }, [machineId, q])
  useEffect(() => { const t = setTimeout(load, 250); return () => clearTimeout(t) }, [load])

  async function run(fn: () => Promise<unknown>, ok?: string) {
    try { await fn(); if (ok) addToast(ok, 'success'); setEditing(null); load() } catch (e) { addToast((e as Error).message, 'error') }
  }

  if (machineId && list.length === 0 && !canAdd && !q) return null

  return (
    <div className="rounded-xl p-4 space-y-3" style={card}>
      <div className="flex items-center justify-between gap-2 flex-wrap">
        <h2 className="text-sm font-semibold flex items-center gap-2" style={{ color: 'var(--text-primary)' }}>
          <SlidersHorizontal size={16} /> Parametros de corte
        </h2>
        <div className="flex gap-2">
          <input value={q} onChange={e => setQ(e.target.value)} placeholder="Buscar material" className="rounded-lg px-2 py-1 text-xs" style={input} />
          {canAdd && !editing && (
            <button onClick={() => setEditing('new')} className="flex items-center gap-1 px-2 py-1 rounded-lg text-xs" style={{ backgroundColor: 'var(--bg-tertiary)', color: 'var(--text-primary)' }}>
              <Plus size={13} /> Agregar
            </button>
          )}
        </div>
      </div>
      {editing === 'new' && machineId && <ParamForm initial={EMPTY} units={units} onCancel={() => setEditing(null)} onSave={d => run(() => createCutParam(machineId, d), 'Parametros guardados')} />}
      {list.length === 0 && editing !== 'new' && (
        <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>
          {q ? 'Nada con ese material.' : 'Todavia no hay parametros. Cuando un corte salga bien, anota aqui potencia, velocidad y pasadas para que otros no tengan que probar de cero.'}
        </p>
      )}
      <div className="space-y-2">
        {list.map(p => editing !== 'new' && editing?.id === p.id ? (
          <ParamForm key={p.id} initial={p} units={units} onCancel={() => setEditing(null)} onSave={d => run(() => updateCutParam(p.id, d), 'Guardado')} />
        ) : (
          <div key={p.id} className="flex items-start justify-between gap-3 p-2 rounded-lg text-sm" style={{ backgroundColor: 'var(--bg-secondary)', borderLeft: `3px solid ${RESULT_COLOR[p.result]}` }}>
            <div className="min-w-0">
              <div style={{ color: 'var(--text-primary)' }}>
                <b>{p.material}</b>{p.thickness_mm != null && ` ${p.thickness_mm} mm`} · {p.operation}
                {!machineId && <span style={{ color: 'var(--text-secondary)' }}> · {p.machine_name}</span>}
              </div>
              <div className="text-xs font-mono" style={{ color: 'var(--text-primary)' }}>
                {p.power != null && `potencia ${p.power} `}{p.speed != null && `· velocidad ${p.speed} `}{p.passes != null && `· ${p.passes} pasada${p.passes > 1 ? 's' : ''}`}
              </div>
              {p.notes && <div className="text-xs whitespace-pre-wrap" style={{ color: 'var(--text-secondary)' }}>{p.notes}</div>}
              <div className="text-[11px]" style={{ color: 'var(--text-secondary)' }}>
                {p.created_by} · {p.created_at.slice(0, 10)}{p.result !== 'bueno' && <span style={{ color: RESULT_COLOR[p.result] }}> · resultado {p.result}</span>}
                {p.photo_id && <> · <a href={machinePhotoUrl(p.photo_id)} target="_blank" rel="noreferrer" className="underline" style={{ color: 'var(--accent)' }}>foto</a></>}
              </div>
            </div>
            <div className="flex items-center gap-1 shrink-0">
              <button onClick={() => run(() => voteCutParam(p.id))} title="Me funciono" className="flex items-center gap-1 px-2 py-0.5 rounded text-xs"
                style={{ color: p.voted ? 'var(--accent)' : 'var(--text-secondary)', border: '1px solid var(--border)' }}>
                <ThumbsUp size={12} /> {p.votes}
              </button>
              {(staff || p.created_by === user?.username) && (
                <>
                  <button onClick={() => setEditing(p)} className="p-1" title="Editar"><Pencil size={13} style={{ color: 'var(--text-secondary)' }} /></button>
                  <button onClick={() => confirm('Borrar estos parametros?') && run(() => deleteCutParam(p.id), 'Borrado')} className="p-1" title="Borrar"><Trash2 size={13} style={{ color: 'var(--danger)' }} /></button>
                </>
              )}
            </div>
          </div>
        ))}
      </div>
    </div>
  )
}

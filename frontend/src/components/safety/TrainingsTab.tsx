import { useEffect, useState } from 'react'
import { Loader2, Plus, Trash2, X } from 'lucide-react'
import {
  createTraining,
  deleteTraining,
  fetchTrainingRecords,
  fetchUsernames,
  grantTraining,
  revokeTraining,
  updateTraining,
  type MyTraining,
  type SafetyTraining,
  type TrainingRecord,
} from '../../api'
import { useToast } from '../ToastContext'
import { errorMessage } from '../../lib/errors'
import { TRAINING_STATUS, cardStyle, fmtDate, inputStyle } from '../../lib/safety'

function MyList({ mine }: { mine: MyTraining[] }) {
  return (
    <div className="rounded-xl p-4 space-y-2" style={cardStyle}>
      <h2 className="text-sm font-semibold" style={{ color: 'var(--text-primary)' }}>Mis capacitaciones</h2>
      {mine.length === 0 ? (
        <p className="text-sm" style={{ color: 'var(--text-secondary)' }}>Todavia no hay capacitaciones en el catalogo.</p>
      ) : mine.map((m) => {
        const st = TRAINING_STATUS[m.status]
        return (
          <div key={m.training.id} className="flex items-center gap-2 text-sm px-3 py-2 rounded-lg flex-wrap" style={{ backgroundColor: 'var(--bg-secondary)' }}>
            <span className="font-medium" style={{ color: 'var(--text-primary)' }}>{m.training.name}</span>
            {m.training.mandatory && <span className="text-[11px]" style={{ color: 'var(--text-secondary)' }}>obligatoria</span>}
            <span className="text-[11px] px-2 py-0.5 rounded-full" style={{ color: st.color, border: '1px solid currentColor' }}>{st.label}</span>
            <span className="text-xs ml-auto" style={{ color: 'var(--text-secondary)' }}>
              {m.granted_on && `hecha ${fmtDate(m.granted_on)}`}{m.expires_on && ` · vence ${fmtDate(m.expires_on)}`}
            </span>
          </div>
        )
      })}
    </div>
  )
}

function TrainingForm({ initial, onDone, onCancel }: { initial: SafetyTraining | null; onDone: () => void; onCancel: () => void }) {
  const { addToast } = useToast()
  const [name, setName] = useState(initial?.name ?? '')
  const [description, setDescription] = useState(initial?.description ?? '')
  const [months, setMonths] = useState(initial?.valid_months ? String(initial.valid_months) : '')
  const [mandatory, setMandatory] = useState(initial?.mandatory ?? false)
  const [busy, setBusy] = useState(false)

  const save = async () => {
    setBusy(true)
    try {
      const data = { name, description, valid_months: months ? Number(months) : null, mandatory }
      if (initial) await updateTraining(initial.id, data)
      else await createTraining(data)
      addToast('Capacitacion guardada', 'success')
      onDone()
    } catch (e) {
      addToast(errorMessage(e), 'error')
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="rounded-xl p-4 space-y-3" style={cardStyle}>
      <h3 className="text-sm font-semibold" style={{ color: 'var(--text-primary)' }}>{initial ? 'Editar capacitacion' : 'Nueva capacitacion'}</h3>
      <input value={name} onChange={(e) => setName(e.target.value)} maxLength={120} placeholder="Nombre (p. ej. Uso de extintores)" className="w-full px-3 py-1.5 rounded-lg text-sm outline-none" style={inputStyle} />
      <textarea value={description} onChange={(e) => setDescription(e.target.value)} maxLength={2000} rows={2} placeholder="Descripcion" className="w-full px-3 py-2 rounded-lg text-sm outline-none" style={inputStyle} />
      <div className="flex items-center gap-4 flex-wrap">
        <label className="text-xs" style={{ color: 'var(--text-secondary)' }}>Vigencia en meses (vacio = no vence)
          <input type="number" min={1} max={240} value={months} onChange={(e) => setMonths(e.target.value)} className="block mt-1 w-28 px-2 py-1.5 rounded-lg text-sm outline-none" style={inputStyle} />
        </label>
        <label className="flex items-center gap-2 text-sm" style={{ color: 'var(--text-primary)' }}>
          <input type="checkbox" checked={mandatory} onChange={(e) => setMandatory(e.target.checked)} /> Obligatoria
        </label>
        <div className="ml-auto flex gap-2">
          <button onClick={onCancel} className="px-3 py-1.5 rounded-lg text-sm" style={inputStyle}>Cancelar</button>
          <button disabled={busy || !name.trim()} onClick={save} className="flex items-center gap-1.5 px-4 py-1.5 rounded-lg text-sm font-medium" style={{ backgroundColor: 'var(--accent)', color: '#fff' }}>
            {busy && <Loader2 size={14} className="animate-spin" />} Guardar
          </button>
        </div>
      </div>
    </div>
  )
}

function TrainingRow({ t, onChange }: { t: SafetyTraining; onChange: () => void }) {
  const { addToast } = useToast()
  const [open, setOpen] = useState(false)
  const [editing, setEditing] = useState(false)
  const [records, setRecords] = useState<TrainingRecord[]>([])
  const [users, setUsers] = useState<string[]>([])
  const [who, setWho] = useState('')
  const [granted, setGranted] = useState('')
  const [expires, setExpires] = useState('')
  const [busy, setBusy] = useState(false)

  const loadRecords = () => fetchTrainingRecords(t.id).then(setRecords).catch((e) => addToast(errorMessage(e), 'error'))
  useEffect(() => {
    if (!open) return
    loadRecords()
    fetchUsernames().then(setUsers).catch(() => {})
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, t.id, t.holders])

  const run = async (fn: () => Promise<unknown>, ok: string) => {
    setBusy(true)
    try {
      await fn()
      addToast(ok, 'success')
      onChange()
      await loadRecords()
    } catch (e) {
      addToast(errorMessage(e), 'error')
    } finally {
      setBusy(false)
    }
  }

  if (editing) return <TrainingForm initial={t} onDone={() => { setEditing(false); onChange() }} onCancel={() => setEditing(false)} />

  return (
    <div className="rounded-xl p-4 space-y-2" style={cardStyle}>
      <div className="flex items-center gap-2 flex-wrap">
        <span className="text-sm font-semibold" style={{ color: 'var(--text-primary)' }}>{t.name}</span>
        {t.mandatory && <span className="text-[11px] px-2 py-0.5 rounded-full" style={{ color: 'var(--danger)', border: '1px solid currentColor' }}>Obligatoria</span>}
        <span className="text-xs" style={{ color: 'var(--text-secondary)' }}>
          {t.valid_months ? `vigencia ${t.valid_months} meses` : 'no vence'} · {t.holders} {t.holders === 1 ? 'persona vigente' : 'personas vigentes'}
        </span>
        <div className="ml-auto flex gap-3 text-xs">
          <button onClick={() => setOpen(!open)} style={{ color: 'var(--accent)' }}>{open ? 'Ocultar' : 'Registros'}</button>
          <button onClick={() => setEditing(true)} style={{ color: 'var(--accent)' }}>Editar</button>
          <button onClick={() => { if (confirm(`Eliminar "${t.name}" y todos sus registros?`)) run(() => deleteTraining(t.id), 'Capacitacion eliminada') }} style={{ color: 'var(--danger)' }} aria-label="Eliminar"><Trash2 size={14} /></button>
        </div>
      </div>
      {t.description && <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>{t.description}</p>}
      {open && (
        <div className="space-y-2 pt-2" style={{ borderTop: '1px solid var(--border)' }}>
          {records.length === 0 ? (
            <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>Nadie la tiene registrada.</p>
          ) : records.map((r) => (
            <div key={r.username} className="flex items-center gap-2 text-sm px-3 py-1.5 rounded-lg flex-wrap" style={{ backgroundColor: 'var(--bg-secondary)' }}>
              <span className="font-medium" style={{ color: 'var(--text-primary)' }}>{r.username}</span>
              <span className="text-[11px]" style={{ color: r.status === 'vigente' ? 'var(--success)' : 'var(--danger)' }}>{r.status === 'vigente' ? 'Vigente' : 'Vencida'}</span>
              <span className="text-xs flex-1 min-w-0 truncate" style={{ color: 'var(--text-secondary)' }}>
                {fmtDate(r.granted_on)}{r.expires_on && ` · vence ${fmtDate(r.expires_on)}`} · {r.granted_by}{r.notes && ` · ${r.notes}`}
              </span>
              <button disabled={busy} onClick={() => run(() => revokeTraining(t.id, r.username), 'Registro quitado')} style={{ color: 'var(--danger)' }} aria-label="Quitar"><X size={14} /></button>
            </div>
          ))}
          <div className="flex items-end gap-2 flex-wrap">
            <label className="text-xs flex-1 min-w-[9rem]" style={{ color: 'var(--text-secondary)' }}>Persona
              <select value={who} onChange={(e) => setWho(e.target.value)} className="block w-full mt-1 px-2 py-1.5 rounded-lg text-sm outline-none" style={inputStyle}>
                <option value="">Elegir...</option>
                {users.map((u) => <option key={u} value={u}>{u}</option>)}
              </select>
            </label>
            <label className="text-xs" style={{ color: 'var(--text-secondary)' }}>Fecha (hoy si vacio)
              <input type="date" value={granted} onChange={(e) => setGranted(e.target.value)} className="block mt-1 px-2 py-1.5 rounded-lg text-sm outline-none" style={inputStyle} />
            </label>
            <label className="text-xs" style={{ color: 'var(--text-secondary)' }}>Vence (auto si hay vigencia)
              <input type="date" value={expires} onChange={(e) => setExpires(e.target.value)} className="block mt-1 px-2 py-1.5 rounded-lg text-sm outline-none" style={inputStyle} />
            </label>
            <button disabled={busy || !who} onClick={() => run(() => grantTraining({ training_id: t.id, username: who, granted_on: granted || null, expires_on: expires || null }), `Registrada a ${who}`).then(() => setWho(''))}
              className="px-4 py-2 rounded-lg text-sm font-medium" style={{ backgroundColor: 'var(--accent)', color: '#fff', opacity: !who ? 0.6 : 1 }}>
              Registrar
            </button>
          </div>
        </div>
      )}
    </div>
  )
}

interface Props {
  trainings: SafetyTraining[]
  mine: MyTraining[]
  staff: boolean
  onChange: () => void
}

/** Capacitaciones generales: las mias (todos) y el catalogo con sus registros (Gestionar) */
export default function TrainingsTab({ trainings, mine, staff, onChange }: Props) {
  const [creating, setCreating] = useState(false)
  return (
    <div className="space-y-4">
      <MyList mine={mine} />
      {staff && (
        <div className="space-y-3">
          <div className="flex items-center">
            <h2 className="text-sm font-semibold" style={{ color: 'var(--text-primary)' }}>Catalogo y registros</h2>
            <button onClick={() => setCreating(true)} className="ml-auto flex items-center gap-1.5 px-4 py-1.5 rounded-lg text-sm font-medium" style={{ backgroundColor: 'var(--accent)', color: '#fff' }}>
              <Plus size={14} /> Nueva
            </button>
          </div>
          {creating && <TrainingForm initial={null} onDone={() => { setCreating(false); onChange() }} onCancel={() => setCreating(false)} />}
          {trainings.map((t) => <TrainingRow key={t.id} t={t} onChange={onChange} />)}
        </div>
      )}
    </div>
  )
}

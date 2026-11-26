import { useCallback, useEffect, useState } from 'react'
import { Loader2, Play, Square, Timer } from 'lucide-react'
import {
  fetchMachineUsage,
  shortDateTime,
  startMachineUsage,
  stopMachineUsage,
  type MachineDetail,
  type MachineUsageList,
} from '../../api'
import { useToast } from '../ToastContext'

const inputStyle = { backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }
const card = { backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }

interface Props {
  machine: MachineDetail
  me: string
  staff: boolean
  onChange: () => void
}

/** Uso real de la maquina: empezar y terminar una sesion, y las ultimas sesiones */
export default function UsagePanel({ machine, me, staff, onChange }: Props) {
  const { addToast } = useToast()
  const [data, setData] = useState<MachineUsageList | null>(null)
  const [busy, setBusy] = useState(false)
  const [project, setProject] = useState('')
  const [note, setNote] = useState('')

  const load = useCallback(() => {
    fetchMachineUsage(machine.id).then(setData).catch(() => {})
  }, [machine.id])

  useEffect(() => { load() }, [load, machine.in_use?.id])

  const open = data?.open ?? null
  const mine = open?.username === me
  const retired = machine.status === 'retirada'

  const run = async (fn: () => Promise<unknown>, ok: string) => {
    setBusy(true)
    try {
      await fn()
      addToast(ok, 'success')
      setNote('')
      load()
      onChange()
    } catch (e) {
      addToast((e as Error).message, 'error')
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="rounded-xl p-4 space-y-3" style={card}>
      <h2 className="text-sm font-semibold flex items-center gap-2" style={{ color: 'var(--text-primary)' }}>
        <Timer size={16} /> Uso
      </h2>

      {open ? (
        <div className="space-y-2">
          <p className="text-sm" style={{ color: 'var(--text-primary)' }}>
            {mine ? 'La estas usando' : <>La esta usando <b>{open.username}</b></>} desde {shortDateTime(open.started_at)}
            {open.project && <> · {open.project}</>}
          </p>
          {(mine || staff) && (
            <>
              <div className="grid grid-cols-1 sm:grid-cols-2 gap-2">
                <input value={project} onChange={(e) => setProject(e.target.value)} maxLength={120}
                  placeholder={open.project ? `Proyecto (${open.project})` : 'Proyecto (opcional)'}
                  className="rounded-lg px-2 py-1.5 text-sm" style={inputStyle} />
                <input value={note} onChange={(e) => setNote(e.target.value)} maxLength={500} placeholder="Nota (opcional)"
                  className="rounded-lg px-2 py-1.5 text-sm" style={inputStyle} />
              </div>
              <button disabled={busy} onClick={() => run(() => stopMachineUsage(open.id, note, project.trim() || open.project), 'Sesion terminada')}
                className="w-full flex items-center justify-center gap-2 px-4 py-3 rounded-xl text-base font-medium disabled:opacity-60"
                style={{ backgroundColor: 'var(--danger)', color: '#fff' }}>
                {busy ? <Loader2 size={18} className="animate-spin" /> : <Square size={18} />} Terminar{!mine && ` la sesion de ${open.username}`}
              </button>
            </>
          )}
        </div>
      ) : !retired && (
        <div className="space-y-2">
          <input value={project} onChange={(e) => setProject(e.target.value)} maxLength={120} placeholder="Proyecto (opcional)"
            className="w-full rounded-lg px-2 py-1.5 text-sm" style={inputStyle} />
          <button disabled={busy} onClick={() => run(() => startMachineUsage(machine.id, project.trim()), 'Sesion iniciada')}
            className="w-full flex items-center justify-center gap-2 px-4 py-3 rounded-xl text-base font-medium disabled:opacity-60"
            style={{ backgroundColor: 'var(--accent)', color: '#fff' }}>
            {busy ? <Loader2 size={18} className="animate-spin" /> : <Play size={18} />} Empezar a usar
          </button>
        </div>
      )}

      {data && data.recent.length > 0 && (
        <ul className="divide-y text-xs" style={{ borderColor: 'var(--border)' }}>
          {data.recent.slice(0, 8).map((s) => (
            <li key={s.id} className="py-1.5 flex items-center justify-between gap-2 flex-wrap" style={{ borderColor: 'var(--border)' }}>
              <span style={{ color: 'var(--text-primary)' }}>
                {shortDateTime(s.started_at)} · {s.username}{s.project && ` · ${s.project}`}
                {s.note && <span style={{ color: 'var(--text-secondary)' }}> · {s.note}</span>}
              </span>
              <span style={{ color: 'var(--text-secondary)' }}>{s.hours ?? 0} h</span>
            </li>
          ))}
        </ul>
      )}
    </div>
  )
}

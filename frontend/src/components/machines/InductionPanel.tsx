import { useEffect, useState } from 'react'
import { Check, GraduationCap, Loader2, X } from 'lucide-react'
import {
  fetchUsernames,
  grantCertification,
  requestCertification,
  revokeCertification,
  type MachineCertification,
  type MachineDetail,
} from '../../api'
import { useToast } from '../ToastContext'

const inputStyle = { backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }
const card = { backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }

const STATUS: Record<MachineCertification['status'], { label: string; color: string }> = {
  vigente: { label: 'Vigente', color: 'var(--success)' },
  solicitada: { label: 'La pidio', color: 'var(--warning, #d97706)' },
  vencida: { label: 'Vencida', color: 'var(--danger)' },
}

function fmtDate(ymd: string): string {
  const [y, m, d] = ymd.split('-')
  return `${d}-${m}-${y}`
}

interface Props {
  machine: MachineDetail
  certs: MachineCertification[]
  me: string
  staff: boolean
  onChange: () => void
}

/** Induccion de la maquina: pedirla (usuarios) o darla y quitarla (staff) */
export default function InductionPanel({ machine, certs, me, staff, onChange }: Props) {
  const { addToast } = useToast()
  const [busy, setBusy] = useState(false)
  const [note, setNote] = useState('')
  const [users, setUsers] = useState<string[]>([])
  const [who, setWho] = useState('')
  const [expires, setExpires] = useState('')

  useEffect(() => {
    if (staff) fetchUsernames().then(setUsers).catch(() => {})
  }, [staff])

  if (!machine.requires_certification && certs.length === 0) return null

  const run = async (fn: () => Promise<unknown>, ok: string) => {
    setBusy(true)
    try {
      await fn()
      addToast(ok, 'success')
      onChange()
      return true
    } catch (e) {
      addToast((e as Error).message, 'error')
      return false
    } finally {
      setBusy(false)
    }
  }

  const mine = certs.find((c) => c.username === me)
  const id = machine.id

  return (
    <div className="rounded-xl p-4 space-y-3" style={card}>
      <h2 className="text-sm font-semibold flex items-center gap-2" style={{ color: 'var(--text-primary)' }}>
        <GraduationCap size={16} /> Induccion
        {!machine.requires_certification && <span className="text-xs font-normal" style={{ color: 'var(--text-secondary)' }}>(ya no se exige para reservar)</span>}
      </h2>

      {!staff && (
        mine?.status === 'vigente' ? (
          <p className="text-sm" style={{ color: 'var(--success)' }}>
            ✓ Tienes la induccion{mine.expires_on ? `, vigente hasta el ${fmtDate(mine.expires_on)}` : ''}.
          </p>
        ) : mine?.status === 'solicitada' ? (
          <div className="flex items-center gap-3 flex-wrap text-sm">
            <span style={{ color: 'var(--text-primary)' }}>
              Pediste la induccion. {machine.responsible ? `${machine.responsible} o un operador` : 'Un operador'} te avisara para coordinarla.
            </span>
            <button disabled={busy} onClick={() => run(() => revokeCertification(id, me), 'Solicitud retirada')} className="text-xs" style={{ color: 'var(--danger)' }}>
              Retirar solicitud
            </button>
          </div>
        ) : (
          <div className="space-y-2">
            <p className="text-sm" style={{ color: 'var(--text-primary)' }}>
              {mine?.status === 'vencida'
                ? `Tu induccion vencio el ${fmtDate(mine.expires_on ?? '')}. Pidela de nuevo para volver a reservar.`
                : 'Para reservar esta maquina necesitas hacer la induccion con el responsable.'}
            </p>
            <div className="flex gap-2 flex-wrap">
              <input value={note} onChange={(e) => setNote(e.target.value)} maxLength={500} placeholder="Cuando puedes, para que la necesitas (opcional)"
                className="flex-1 min-w-[12rem] px-3 py-1.5 rounded-lg text-sm outline-none" style={inputStyle} />
              <button disabled={busy} onClick={async () => { if (await run(() => requestCertification(id, note), 'Induccion pedida')) setNote('') }}
                className="flex items-center gap-1.5 px-4 py-1.5 rounded-lg text-sm font-medium" style={{ backgroundColor: 'var(--accent)', color: '#fff' }}>
                {busy && <Loader2 size={14} className="animate-spin" />} Pedir induccion
              </button>
            </div>
          </div>
        )
      )}

      {staff && (
        <>
          {certs.length === 0 ? (
            <p className="text-sm" style={{ color: 'var(--text-secondary)' }}>Nadie tiene la induccion todavia.</p>
          ) : (
            <div className="space-y-1.5">
              {certs.map((c) => {
                const st = STATUS[c.status]
                return (
                  <div key={c.username} className="flex items-center gap-2 text-sm px-3 py-2 rounded-lg flex-wrap" style={{ backgroundColor: 'var(--bg-secondary)' }}>
                    <span className="font-medium" style={{ color: 'var(--text-primary)' }}>{c.username}</span>
                    <span className="text-[11px] px-2 py-0.5 rounded-full" style={{ color: st.color, border: '1px solid currentColor' }}>{st.label}</span>
                    <span className="flex-1 min-w-0 truncate text-xs" style={{ color: 'var(--text-secondary)' }}>
                      {c.expires_on && `vence ${fmtDate(c.expires_on)}`}
                      {c.granted_by && c.status !== 'solicitada' && ` · dada por ${c.granted_by}`}
                      {c.notes && ` · ${c.notes}`}
                    </span>
                    {c.status !== 'vigente' && (
                      <button disabled={busy} onClick={() => run(() => grantCertification(id, { username: c.username, expires_on: expires || null }), `Induccion dada a ${c.username}`)}
                        className="flex items-center gap-1 text-xs px-2 py-0.5 rounded" style={{ color: 'var(--success)' }} title="Usa el vencimiento de abajo si lo pusiste">
                        <Check size={13} /> {c.status === 'vencida' ? 'Renovar' : 'Ya la hizo'}
                      </button>
                    )}
                    <button disabled={busy} onClick={() => run(() => revokeCertification(id, c.username), c.status === 'solicitada' ? 'Solicitud rechazada' : 'Induccion quitada')}
                      className="flex items-center gap-1 text-xs px-2 py-0.5 rounded" style={{ color: 'var(--danger)' }}>
                      <X size={13} /> {c.status === 'solicitada' ? 'Rechazar' : 'Quitar'}
                    </button>
                  </div>
                )
              })}
            </div>
          )}
          <div className="flex items-end gap-2 flex-wrap pt-2" style={{ borderTop: '1px solid var(--border)' }}>
            <label className="text-xs flex-1 min-w-[9rem]" style={{ color: 'var(--text-secondary)' }}>Persona
              <select value={who} onChange={(e) => setWho(e.target.value)} className="block w-full mt-1 px-2 py-1.5 rounded-lg text-sm outline-none" style={inputStyle}>
                <option value="">Elegir...</option>
                {users.filter((u) => !certs.some((c) => c.username === u && c.status === 'vigente')).map((u) => <option key={u} value={u}>{u}</option>)}
              </select>
            </label>
            <label className="text-xs" style={{ color: 'var(--text-secondary)' }}>Vence (opcional)
              <input type="date" value={expires} onChange={(e) => setExpires(e.target.value)} className="block mt-1 px-2 py-1.5 rounded-lg text-sm outline-none" style={inputStyle} />
            </label>
            <button disabled={busy || !who} onClick={async () => { if (await run(() => grantCertification(id, { username: who, expires_on: expires || null }), `Induccion dada a ${who}`)) setWho('') }}
              className="flex items-center gap-1.5 px-4 py-2 rounded-lg text-sm font-medium" style={{ backgroundColor: 'var(--accent)', color: '#fff', opacity: !who ? 0.6 : 1 }}>
              Dar induccion
            </button>
          </div>
        </>
      )}
    </div>
  )
}

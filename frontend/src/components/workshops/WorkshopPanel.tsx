import { useCallback, useEffect, useState } from 'react'
import { Loader2, UserPlus, UserMinus, Download, Award, Copy, Link2, RefreshCw } from 'lucide-react'
import {
  fetchWorkshop, saveWorkshop, enrollWorkshop, cancelWorkshopEnrollment, myWorkshopCertificate,
  setWorkshopAttendance, cancelWorkshopRegistration, issueWorkshopCertificate, workshopCsvUrl,
  workshopPublicUrl, type WorkshopInfo, type WorkshopReg,
} from '../../api'
import { printCertificate } from '../../lib/certificate'

const label = { color: 'var(--text-secondary)' }
const input = { backgroundColor: 'var(--bg-secondary)', border: '1px solid var(--border)', color: 'var(--text-primary)' }
const btn = 'flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium transition-all hover:opacity-80'

/** Inscripciones de un taller o curso del Portafolio */
export default function WorkshopPanel({ entryId, canUse }: { entryId: string; canUse: boolean }) {
  const [info, setInfo] = useState<WorkshopInfo | null>(null)
  const [error, setError] = useState('')
  const [busy, setBusy] = useState(false)
  const [capacity, setCapacity] = useState(0)
  const [pct, setPct] = useState(75)
  const [sessions, setSessions] = useState('')
  const [open, setOpen] = useState(false)

  const apply = useCallback((i: WorkshopInfo) => {
    setInfo(i)
    setCapacity(i.capacity)
    setPct(i.min_attendance)
    setSessions(i.sessions.join('\n'))
    setOpen(i.registration_open)
  }, [])

  useEffect(() => { fetchWorkshop(entryId).then(apply).catch(e => setError(e.message)) }, [entryId, apply])

  async function run(fn: () => Promise<unknown>) {
    setBusy(true); setError('')
    try { await fn(); apply(await fetchWorkshop(entryId)) } catch (e) { setError(e instanceof Error ? e.message : 'Error') }
    setBusy(false)
  }

  const save = (rotate = false, openNow = open) => run(async () => {
    await saveWorkshop(entryId, {
      capacity, registration_open: openNow, min_attendance: pct, rotate_token: rotate,
      sessions: sessions.split('\n').map(s => s.trim()).filter(Boolean),
    })
  })

  const toggleSession = (r: WorkshopReg, s: string) => run(() =>
    setWorkshopAttendance(entryId, r.id, r.attended.includes(s) ? r.attended.filter(a => a !== s) : [...r.attended, s]))

  if (!info) {
    return error ? <p className="text-xs text-red-500">{error}</p> : <Loader2 size={16} className="animate-spin" style={{ color: 'var(--accent)' }} />
  }
  const mine = info.mine
  const link = info.public_token ? workshopPublicUrl(info.public_token) : ''

  return (
    <div className="space-y-3 rounded-lg p-3" style={{ backgroundColor: 'var(--bg-secondary)' }}>
      <div className="flex items-center gap-2 flex-wrap">
        <label className="text-[11px] font-medium uppercase" style={label}>Inscripciones</label>
        <span className="text-xs" style={label}>
          {info.registration_open ? 'Abiertas' : 'Cerradas'} · {info.enrolled}{info.capacity > 0 ? `/${info.capacity}` : ''} inscritos
          {info.waitlist > 0 ? ` · ${info.waitlist} en espera` : ''}
        </span>
        <span className="flex-1" />
        {canUse && !mine && (
          <button className={btn} disabled={busy || !info.registration_open} style={{ backgroundColor: 'var(--accent)', color: '#fff' }}
            onClick={() => run(() => enrollWorkshop(entryId))}>
            <UserPlus size={13} /> Inscribirme
          </button>
        )}
        {mine && (
          <>
            <span className="text-xs" style={{ color: 'var(--text-primary)' }}>
              Tu inscripcion: {mine.status === 'espera' ? 'lista de espera' : 'inscrito'}
            </span>
            {mine.status === 'inscrito' && info.sessions.length > 0 && (
              <button className={btn} disabled={busy} style={{ backgroundColor: 'var(--card-bg)', color: 'var(--text-primary)' }}
                onClick={() => run(async () => printCertificate(await myWorkshopCertificate(entryId)))}>
                <Award size={13} /> Mi certificado
              </button>
            )}
            <button className={btn} disabled={busy} style={{ backgroundColor: 'var(--card-bg)', color: 'var(--text-primary)' }}
              onClick={() => run(() => cancelWorkshopEnrollment(entryId))}>
              <UserMinus size={13} /> Cancelar
            </button>
          </>
        )}
      </div>
      {error && <p className="text-xs text-red-500">{error}</p>}

      {info.can_manage && (
        <div className="space-y-3">
          <div className="grid grid-cols-2 gap-3">
            <div>
              <label className="text-[11px]" style={label}>Cupo (0 = sin limite)</label>
              <input type="number" min={0} className="w-full mt-1 px-2 py-1.5 rounded-lg text-sm" style={input}
                value={capacity} onChange={e => setCapacity(Number(e.target.value))} />
            </div>
            <div>
              <label className="text-[11px]" style={label}>Asistencia minima para certificado (%)</label>
              <input type="number" min={1} max={100} className="w-full mt-1 px-2 py-1.5 rounded-lg text-sm" style={input}
                value={pct} onChange={e => setPct(Number(e.target.value))} />
            </div>
            <div className="col-span-2">
              <label className="text-[11px]" style={label}>Sesiones (una por linea, ej. 2026-12-01)</label>
              <textarea rows={3} className="w-full mt-1 px-2 py-1.5 rounded-lg text-sm" style={input}
                value={sessions} onChange={e => setSessions(e.target.value)} />
            </div>
          </div>
          <div className="flex items-center gap-2 flex-wrap">
            <button className={btn} disabled={busy} style={{ backgroundColor: 'var(--accent)', color: '#fff' }} onClick={() => save()}>Guardar</button>
            <button className={btn} disabled={busy} style={{ backgroundColor: 'var(--card-bg)', color: 'var(--text-primary)' }}
              onClick={() => { setOpen(!open); save(false, !open) }}>
              {open ? 'Cerrar inscripcion' : 'Abrir inscripcion'}
            </button>
            <a className={btn} style={{ backgroundColor: 'var(--card-bg)', color: 'var(--text-primary)' }} href={workshopCsvUrl(entryId)}>
              <Download size={13} /> CSV
            </a>
          </div>
          {link && (
            <div className="flex items-center gap-2 text-xs flex-wrap" style={label}>
              <Link2 size={13} />
              <code className="break-all">{link}</code>
              <button title="Copiar" onClick={() => navigator.clipboard?.writeText(link)}><Copy size={13} /></button>
              <button title="Cambiar el enlace (el anterior deja de servir)" disabled={busy} onClick={() => save(true)}><RefreshCw size={13} /></button>
            </div>
          )}

          <div className="space-y-1">
            {(info.registrations ?? []).filter(r => r.status !== 'cancelado').map(r => (
              <div key={r.id} className="rounded-lg p-2 text-xs space-y-1" style={{ backgroundColor: 'var(--card-bg)' }}>
                <div className="flex items-center gap-2 flex-wrap">
                  <span className="font-medium" style={{ color: 'var(--text-primary)' }}>{r.name}</span>
                  <span style={label}>{r.email || r.username} {r.affiliation && `· ${r.affiliation}`}</span>
                  {r.status === 'espera' && <span className="px-1.5 rounded" style={{ backgroundColor: 'var(--bg-secondary)', color: 'var(--warning)' }}>espera</span>}
                  <span className="flex-1" />
                  {r.status === 'inscrito' && info.sessions.length > 0 && (
                    <button className={btn} disabled={busy} style={{ backgroundColor: 'var(--bg-secondary)', color: 'var(--text-primary)' }}
                      onClick={() => run(async () => printCertificate(await issueWorkshopCertificate(entryId, r.id)))}>
                      <Award size={12} /> {r.certificate_code ? 'Ver certificado' : 'Certificar'}
                    </button>
                  )}
                  <button className={btn} disabled={busy} style={{ backgroundColor: 'var(--bg-secondary)', color: 'var(--danger)' }}
                    onClick={() => { if (confirm(`Cancelar la inscripcion de ${r.name}?`)) run(() => cancelWorkshopRegistration(entryId, r.id)) }}>
                    <UserMinus size={12} />
                  </button>
                </div>
                {r.status === 'inscrito' && info.sessions.length > 0 && (
                  <div className="flex flex-wrap gap-2">
                    {info.sessions.map(s => (
                      <label key={s} className="flex items-center gap-1" style={label}>
                        <input type="checkbox" checked={r.attended.includes(s)} disabled={busy} onChange={() => toggleSession(r, s)} /> {s}
                      </label>
                    ))}
                  </div>
                )}
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  )
}

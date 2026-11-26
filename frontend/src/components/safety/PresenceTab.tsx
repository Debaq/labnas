import { useCallback, useEffect, useState } from 'react'
import { DoorOpen, LogIn, LogOut, QrCode, UserPlus } from 'lucide-react'
import { checkIn, checkOut, fetchDoorQr, fetchPresence, markOut, visitorIn, type PresenceOverview } from '../../api'
import { useAuth } from '../../auth/AuthContext'
import { useEvent } from '../../events/useEvents'
import { useToast } from '../ToastContext'
import { printQrLabel } from '../../lib/qrLabel'
import { errorMessage } from '../../lib/errors'

const card = { backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }
const input = { backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }
const hhmm = (s: string) => s.slice(11, 16)

/** Quien esta en el laboratorio: marcar entrada/salida, visitas y cuantos vienen */
export default function PresenceTab({ staff }: { staff: boolean }) {
  const { user, canModule } = useAuth()
  const { addToast } = useToast()
  const canUse = canModule('seguridad', 'usar')
  const [data, setData] = useState<PresenceOverview | null>(null)
  const [aff, setAff] = useState<string | null>(null)
  const [visitor, setVisitor] = useState<{ name: string; affiliation: string } | null>(null)

  const load = useCallback(() => { fetchPresence().then(setData).catch(() => {}) }, [])
  useEffect(() => { load() }, [load])
  useEvent('safety.presence', load, load)

  async function run(fn: () => Promise<unknown>, ok: string) {
    try { await fn(); addToast(ok, 'success'); load() } catch (e) { addToast(errorMessage(e), 'error') }
  }

  if (!data) return null
  const affiliation = aff ?? data.my_affiliation
  const maxDay = Math.max(1, ...data.by_day.map(([, n]) => n))

  return (
    <div className="space-y-4">
      {canUse && (
        <div className="rounded-xl p-4 space-y-3" style={card}>
          {data.me ? (
            <>
              <p className="text-sm" style={{ color: 'var(--text-primary)' }}>Estas marcado adentro desde las {hhmm(data.me.entered_at)}.</p>
              <button onClick={() => run(checkOut, 'Salida registrada')} className="w-full flex items-center justify-center gap-2 py-3 rounded-xl text-base font-medium" style={{ backgroundColor: 'var(--danger)', color: '#fff' }}>
                <LogOut size={18} /> Sali
              </button>
            </>
          ) : (
            <>
              <input value={affiliation} onChange={e => setAff(e.target.value)} maxLength={80} placeholder="Carrera o afiliacion (opcional)" className="w-full rounded-lg px-3 py-2 text-sm" style={input} />
              <button onClick={() => run(() => checkIn(affiliation), 'Entrada registrada')} className="w-full flex items-center justify-center gap-2 py-3 rounded-xl text-base font-medium" style={{ backgroundColor: 'var(--accent)', color: '#fff' }}>
                <LogIn size={18} /> Entre
              </button>
            </>
          )}
        </div>
      )}

      <div className="rounded-xl p-4 space-y-2" style={card}>
        <div className="flex items-center justify-between gap-2 flex-wrap">
          <h2 className="text-sm font-semibold flex items-center gap-2" style={{ color: 'var(--text-primary)' }}>
            <DoorOpen size={16} /> Adentro ahora ({data.inside.length}) · hoy vinieron {data.today}
          </h2>
          <div className="flex gap-2">
            {canUse && !visitor && (
              <button onClick={() => setVisitor({ name: '', affiliation: '' })} className="flex items-center gap-1 px-2 py-1 rounded-lg text-xs" style={{ backgroundColor: 'var(--bg-tertiary)', color: 'var(--text-primary)' }}>
                <UserPlus size={13} /> Registrar visita
              </button>
            )}
            {staff && (
              <button onClick={async () => { try { printQrLabel(await fetchDoorQr(), 'Registro de entrada', '', 'Escanea al entrar y al salir del laboratorio') } catch (e) { addToast(errorMessage(e), 'error') } }}
                className="flex items-center gap-1 px-2 py-1 rounded-lg text-xs" style={{ backgroundColor: 'var(--bg-tertiary)', color: 'var(--text-primary)' }}>
                <QrCode size={13} /> QR de la puerta
              </button>
            )}
          </div>
        </div>
        {visitor && (
          <div className="flex flex-wrap gap-2">
            <input value={visitor.name} onChange={e => setVisitor({ ...visitor, name: e.target.value })} maxLength={80} placeholder="Nombre de la visita" className="flex-1 min-w-[10rem] rounded-lg px-2 py-1.5 text-sm" style={input} />
            <input value={visitor.affiliation} onChange={e => setVisitor({ ...visitor, affiliation: e.target.value })} maxLength={80} placeholder="De donde viene" className="flex-1 min-w-[10rem] rounded-lg px-2 py-1.5 text-sm" style={input} />
            <button onClick={() => setVisitor(null)} className="px-3 py-1.5 rounded-lg text-sm" style={{ color: 'var(--text-secondary)' }}>Cancelar</button>
            <button onClick={() => run(() => visitorIn(visitor.name, visitor.affiliation), 'Visita registrada').then(() => setVisitor(null))} className="px-3 py-1.5 rounded-lg text-sm" style={{ backgroundColor: 'var(--accent)', color: '#fff' }}>Registrar</button>
          </div>
        )}
        {data.inside.length === 0 && <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>Nadie marcado adentro.</p>}
        <ul className="space-y-1">
          {data.inside.map(e => (
            <li key={e.id} className="flex items-center justify-between gap-2 text-sm px-2 py-1 rounded" style={{ backgroundColor: 'var(--bg-secondary)', color: 'var(--text-primary)' }}>
              <span>
                {e.who}{e.visitor && <span className="text-xs" style={{ color: 'var(--text-secondary)' }}> (visita de {e.registered_by})</span>}
                <span className="text-xs" style={{ color: 'var(--text-secondary)' }}> · desde {hhmm(e.entered_at)}{e.affiliation && ` · ${e.affiliation}`}</span>
              </span>
              {(staff || (e.visitor && e.registered_by === user?.username)) && e.who !== user?.username && (
                <button onClick={() => run(() => markOut(e.id), 'Salida registrada')} className="text-xs" style={{ color: 'var(--accent)' }}>Marcar salida</button>
              )}
            </li>
          ))}
        </ul>
      </div>

      {(data.by_day.length > 0 || data.by_affiliation.length > 0) && (
        <div className="rounded-xl p-4 grid grid-cols-1 md:grid-cols-2 gap-4" style={card}>
          <div>
            <h3 className="text-xs font-semibold mb-2" style={{ color: 'var(--text-secondary)' }}>Personas por dia (14 dias)</h3>
            <div className="flex items-end gap-1 h-24">
              {data.by_day.map(([d, n]) => (
                <div key={d} className="flex-1 rounded-t" title={`${d}: ${n}`} style={{ height: `${(n / maxDay) * 100}%`, backgroundColor: 'var(--accent)', minHeight: 2 }} />
              ))}
            </div>
          </div>
          <div>
            <h3 className="text-xs font-semibold mb-2" style={{ color: 'var(--text-secondary)' }}>Visitas por carrera (30 dias)</h3>
            <ul className="text-xs space-y-0.5">
              {data.by_affiliation.map(([a, n]) => (
                <li key={a} className="flex justify-between" style={{ color: 'var(--text-primary)' }}><span>{a}</span><span>{n}</span></li>
              ))}
            </ul>
          </div>
        </div>
      )}
    </div>
  )
}

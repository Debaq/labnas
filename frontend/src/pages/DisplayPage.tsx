import { useEffect, useState } from 'react'
import { AlertTriangle, Box, CalendarClock, Hourglass, Thermometer, Wrench } from 'lucide-react'
import { fetchDisplay, type DisplayData } from '../api'
import type { IconComponent } from '../lib/icons'
import { READING_LABELS, READING_UNITS } from '../lib/sensors'

const REFRESH_MS = 20_000

const STATE: Record<string, { label: string; color: string }> = {
  libre: { label: 'Libre', color: 'var(--success)' },
  en_uso: { label: 'En uso', color: 'var(--accent)' },
  bloqueada: { label: 'Bloqueada', color: 'var(--text-secondary)' },
  fuera_servicio: { label: 'Fuera de servicio', color: 'var(--danger)' },
}

const hhmm = (s: string | null) => (s ? s.slice(11, 16) : '')

function minutesLeft(until: string | null, now: Date): string {
  if (!until) return ''
  const end = new Date(until)
  const m = Math.max(0, Math.round((end.getTime() - now.getTime()) / 60000))
  return m >= 60 ? `${Math.floor(m / 60)} h ${m % 60} min` : `${m} min`
}

function duration(secs: number | null): string {
  if (secs === null) return ''
  const m = Math.round(secs / 60)
  return m >= 60 ? `${Math.floor(m / 60)} h ${m % 60} min` : `${m} min`
}

function Card({ title, icon: Icon, children, wide }: { title: string; icon: IconComponent; children: React.ReactNode; wide?: boolean }) {
  return (
    <section className={`rounded-2xl p-5 flex flex-col gap-3 min-h-0 ${wide ? 'lg:col-span-2' : ''}`} style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}>
      <h2 className="text-xl font-semibold flex items-center gap-2" style={{ color: 'var(--text-secondary)' }}><Icon size={22} /> {title}</h2>
      <div className="flex flex-col gap-2 overflow-hidden">{children}</div>
    </section>
  )
}

const empty = (t: string) => <p className="text-lg" style={{ color: 'var(--text-secondary)' }}>{t}</p>

/** Pantalla del laboratorio (/pantalla/<token>): sin sesion, se actualiza sola */
export default function DisplayPage({ token }: { token: string }) {
  const [data, setData] = useState<DisplayData | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [now, setNow] = useState(() => new Date())

  useEffect(() => {
    let alive = true
    const load = () => fetchDisplay(token).then(d => { if (alive) { setData(d); setError(null) } }).catch(e => { if (alive) setError((e as Error).message) })
    load()
    const t = setInterval(load, REFRESH_MS)
    const c = setInterval(() => setNow(new Date()), 1000)
    // Que la tablet no se apague
    let lock: { release: () => Promise<void> } | null = null
    const nav = navigator as Navigator & { wakeLock?: { request: (t: 'screen') => Promise<{ release: () => Promise<void> }> } }
    nav.wakeLock?.request('screen').then(l => { lock = l }).catch(() => {})
    return () => { alive = false; clearInterval(t); clearInterval(c); lock?.release().catch(() => {}) }
  }, [token])

  if (!data) {
    return (
      <div className="h-screen flex items-center justify-center text-2xl" style={{ backgroundColor: 'var(--bg-primary)', color: 'var(--text-secondary)' }}>
        {error ?? 'Cargando…'}
      </div>
    )
  }

  const has = (s: string) => data.sections.includes(s as DisplayData['sections'][number])

  return (
    <div className="h-screen flex flex-col gap-4 p-6 overflow-hidden" style={{ backgroundColor: 'var(--bg-primary)' }}>
      <header className="flex items-center justify-between gap-4">
        <div className="flex items-center gap-4 min-w-0">
          {data.logo_url && <img src={data.logo_url} alt="" className="w-14 h-14 rounded-xl object-contain" />}
          <div className="min-w-0">
            <h1 className="text-3xl font-bold truncate" style={{ color: 'var(--text-primary)' }}>{data.lab_name || 'LabNAS'}</h1>
            {error && <p className="text-sm" style={{ color: 'var(--danger)' }}>{error} · mostrando lo ultimo</p>}
          </div>
        </div>
        <div className="text-right">
          <div className="text-5xl font-bold tabular-nums" style={{ color: 'var(--text-primary)' }}>{now.toLocaleTimeString('es-CL', { hour: '2-digit', minute: '2-digit' })}</div>
          <div className="text-lg" style={{ color: 'var(--text-secondary)' }}>{now.toLocaleDateString('es-CL', { weekday: 'long', day: 'numeric', month: 'long' })}</div>
        </div>
      </header>

      {has('avisos') && data.notices.length > 0 && (
        <div className="rounded-2xl px-5 py-3 flex flex-wrap gap-x-6 gap-y-1 text-lg" style={{ backgroundColor: 'color-mix(in srgb, var(--danger) 15%, transparent)', border: '1px solid var(--danger)', color: 'var(--text-primary)' }}>
          {data.notices.slice(0, 6).map((n, i) => (
            <span key={i} className="flex items-center gap-2">
              <AlertTriangle size={18} style={{ color: 'var(--danger)' }} /> {n.text}
            </span>
          ))}
        </div>
      )}

      <main className="flex-1 grid gap-4 grid-cols-1 lg:grid-cols-3 auto-rows-fr min-h-0">
        {has('maquinas') && (
          <Card title="Maquinas" icon={Wrench} wide={data.machines.length > 6}>
            {data.machines.length === 0 && empty('Sin maquinas')}
            <div className={`grid gap-2 ${data.machines.length > 6 ? 'grid-cols-2' : 'grid-cols-1'}`}>
              {data.machines.map(m => (
                <div key={m.name} className="flex items-center justify-between gap-3 px-3 py-2 rounded-xl" style={{ backgroundColor: 'var(--bg-secondary)', borderLeft: `5px solid ${STATE[m.state].color}` }}>
                  <div className="min-w-0">
                    <div className="text-xl font-medium truncate" style={{ color: 'var(--text-primary)' }}>{m.name}</div>
                    <div className="text-sm truncate" style={{ color: 'var(--text-secondary)' }}>
                      {m.state === 'en_uso' ? `${m.user} · quedan ${minutesLeft(m.until, now)}` : m.state === 'bloqueada' ? `hasta las ${hhmm(m.until)}` : m.next_start ? `Proxima ${hhmm(m.next_start)}${m.next_user ? ` · ${m.next_user}` : ''}` : ''}
                    </div>
                  </div>
                  <span className="text-base font-semibold shrink-0" style={{ color: STATE[m.state].color }}>{STATE[m.state].label}</span>
                </div>
              ))}
            </div>
          </Card>
        )}

        {has('impresoras') && (
          <Card title="Impresoras 3D" icon={Box}>
            {data.printers.length === 0 && empty('Sin datos de las impresoras')}
            {data.printers.map(p => (
              <div key={p.name} className="px-3 py-2 rounded-xl space-y-1" style={{ backgroundColor: 'var(--bg-secondary)' }}>
                <div className="flex justify-between gap-2 text-xl" style={{ color: 'var(--text-primary)' }}>
                  <span className="truncate">{p.name}</span>
                  <span className="text-base shrink-0" style={{ color: p.online ? 'var(--text-secondary)' : 'var(--danger)' }}>
                    {!p.online ? 'Apagada' : p.progress !== null && p.file ? `${Math.round(p.progress)}%` : 'Lista'}
                  </span>
                </div>
                {p.online && p.progress !== null && p.file && (
                  <>
                    <div className="h-2 rounded-full overflow-hidden" style={{ backgroundColor: 'var(--bg-tertiary)' }}>
                      <div className="h-full" style={{ width: `${Math.min(100, p.progress)}%`, backgroundColor: 'var(--accent)' }} />
                    </div>
                    <div className="text-sm truncate" style={{ color: 'var(--text-secondary)' }}>
                      {p.file}{p.remaining_secs ? ` · quedan ${duration(p.remaining_secs)}` : ''}
                    </div>
                  </>
                )}
                {p.online && p.hotend !== null && (
                  <div className="text-sm" style={{ color: 'var(--text-secondary)' }}>Boquilla {Math.round(p.hotend)}° · Cama {Math.round(p.bed ?? 0)}°</div>
                )}
              </div>
            ))}
          </Card>
        )}

        {has('agenda') && (
          <Card title="Reservas de hoy" icon={CalendarClock}>
            {data.agenda.length === 0 && empty('Nada mas por hoy')}
            {data.agenda.slice(0, 10).map((a, i) => (
              <div key={i} className="flex items-center gap-3 text-lg" style={{ color: 'var(--text-primary)' }}>
                <span className="tabular-nums w-28 shrink-0" style={{ color: 'var(--text-secondary)' }}>{hhmm(a.start)}–{hhmm(a.end)}</span>
                <span className="truncate">{a.machine} · {a.kind === 'bloqueo' ? 'bloqueado' : a.user}{a.pending ? ' (por aprobar)' : ''}</span>
              </div>
            ))}
          </Card>
        )}

        {has('sensores') && (
          <Card title="Sensores" icon={Thermometer}>
            {data.sensors.length === 0 && empty('Sin sensores')}
            {data.sensors.map(s => (
              <div key={s.name} className="px-3 py-2 rounded-xl" style={{ backgroundColor: 'var(--bg-secondary)', border: s.alarm.length ? '1px solid var(--danger)' : '1px solid transparent' }}>
                <div className="text-lg flex justify-between" style={{ color: 'var(--text-primary)' }}>
                  <span className="truncate">{s.name}</span>
                  {!s.online && <span className="text-sm" style={{ color: 'var(--danger)' }}>sin señal</span>}
                </div>
                <div className="flex flex-wrap gap-x-4 text-2xl font-semibold tabular-nums">
                  {s.readings.filter(([k]) => k !== 'battery' && k !== 'rssi').map(([k, v]) => (
                    <span key={k} style={{ color: s.alarm.includes(k) ? 'var(--danger)' : 'var(--text-primary)' }}>
                      <span className="text-sm font-normal mr-1" style={{ color: 'var(--text-secondary)' }}>{READING_LABELS[k] ?? k}</span>
                      {Math.round(v * 10) / 10}{READING_UNITS[k] ?? ''}
                    </span>
                  ))}
                </div>
              </div>
            ))}
          </Card>
        )}

        {has('plazos') && (
          <Card title="Plazos cercanos" icon={Hourglass}>
            {data.deadlines.length === 0 && empty('Nada en las proximas dos semanas')}
            {data.deadlines.map((d, i) => (
              <div key={i} className="flex items-center justify-between gap-3 text-lg">
                <span className="truncate" style={{ color: 'var(--text-primary)' }}>{d.title}</span>
                <span className="shrink-0 font-semibold" style={{ color: d.days_left < 0 ? 'var(--danger)' : d.days_left <= 7 ? 'var(--warning, #d97706)' : 'var(--text-secondary)' }}>
                  {d.days_left < 0 ? `vencido` : d.days_left === 0 ? 'hoy' : `${d.days_left} d`}
                </span>
              </div>
            ))}
          </Card>
        )}
      </main>
    </div>
  )
}

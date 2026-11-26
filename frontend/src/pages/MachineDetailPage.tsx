import { useCallback, useEffect, useRef, useState } from 'react'
import { Link, useNavigate, useParams } from 'react-router-dom'
import {
  AlertTriangle, ArrowLeft, Camera, Check, ChevronLeft, ChevronRight, Loader2, Lock, MapPin, Pencil, Printer, QrCode, Trash2, User, Wrench, X,
} from 'lucide-react'
import {
  bookingAction,
  compressImage,
  createBooking,
  deleteMachine,
  fetchBookings,
  fetchCertifications,
  fetchMachine,
  fetchMachineQr,
  hhmm,
  kindInfo,
  localDate,
  machinePhotoUrl,
  reportFault,
  resolveFault,
  setMachinePhoto,
  type MachineBooking,
  type MachineCertification,
  type MachineDetail,
  type MachineFault,
} from '../api'
import { useAuth } from '../auth/AuthContext'
import { useEvent } from '../events/useEvents'
import { useToast } from '../components/ToastContext'
import MachineFormModal from '../components/machines/MachineFormModal'
import MachineStatusChip from '../components/machines/MachineStatusChip'
import InductionPanel from '../components/machines/InductionPanel'

const inputStyle = { backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }
const card = { backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }
/** Horas visibles en la linea del dia */
const DAY_START = 7
const DAY_END = 23

function addDays(date: string, n: number): string {
  const d = new Date(`${date}T12:00`)
  d.setDate(d.getDate() + n)
  return localDate(d)
}

function fmtDay(date: string): string {
  const today = localDate(new Date())
  if (date === today) return 'Hoy'
  if (date === addDays(today, 1)) return 'Mañana'
  return new Date(`${date}T12:00`).toLocaleDateString('es-CL', { weekday: 'long', day: 'numeric', month: 'short' })
}

function fmtStamp(iso: string): string {
  const d = new Date(iso)
  return Number.isNaN(d.getTime()) ? iso : d.toLocaleString('es-CL', { dateStyle: 'short', timeStyle: 'short' })
}

/** Proxima media hora desde ahora, `HH:MM` */
function nextHalfHour(): string {
  const d = new Date()
  d.setMinutes(d.getMinutes() < 30 ? 30 : 60, 0, 0)
  return `${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`
}

function plusHour(t: string): string {
  const [h, m] = t.split(':').map(Number)
  return `${String(Math.min(h + 1, 23)).padStart(2, '0')}:${String(h + 1 > 23 ? 59 : m).padStart(2, '0')}`
}

export default function MachineDetailPage() {
  const { id = '' } = useParams()
  const { user, canModule } = useAuth()
  const staff = canModule('machines', 'gestionar')
  const isAdmin = user?.role === 'admin'
  const { addToast } = useToast()
  const navigate = useNavigate()

  const [m, setM] = useState<MachineDetail | null>(null)
  const [missing, setMissing] = useState(false)
  const [day, setDay] = useState(() => localDate(new Date()))
  const [bookings, setBookings] = useState<MachineBooking[]>([])
  const [certs, setCerts] = useState<MachineCertification[]>([])
  const [from, setFrom] = useState(nextHalfHour)
  const [to, setTo] = useState(() => plusHour(nextHalfHour()))
  const [purpose, setPurpose] = useState('')
  const [block, setBlock] = useState(false)
  const [busy, setBusy] = useState(false)
  const [editing, setEditing] = useState(false)
  const [reporting, setReporting] = useState(false)
  const [qr, setQr] = useState<{ url: string; svg: string } | null>(null)
  const photoInput = useRef<HTMLInputElement>(null)

  const load = useCallback(async () => {
    try {
      const [detail, list, c] = await Promise.all([
        fetchMachine(id),
        fetchBookings({ machine: id, from: `${day}T00:00`, to: `${addDays(day, 1)}T00:00` }),
        fetchCertifications(id),
      ])
      setM(detail)
      setBookings(list)
      setCerts(c)
    } catch {
      setMissing(true)
    }
  }, [id, day])

  useEffect(() => { load() }, [load])
  useEvent('machines', load, load)

  const run = async (fn: () => Promise<unknown>, ok: string) => {
    setBusy(true)
    try {
      await fn()
      addToast(ok, 'success')
      await load()
      return true
    } catch (e) {
      addToast((e as Error).message, 'error')
      return false
    } finally {
      setBusy(false)
    }
  }

  const book = async () => {
    const done = await run(
      () => createBooking(id, { start: `${day}T${from}`, end: `${day}T${to}`, purpose, kind: block ? 'bloqueo' : 'reserva' }),
      block ? 'Horario bloqueado' : m?.requires_approval && !staff ? 'Reserva enviada: espera la aprobacion' : 'Reserva lista',
    )
    if (done) { setPurpose(''); setBlock(false) }
  }

  const onPhoto = async (file?: File) => {
    if (!file) return
    try {
      const data = await compressImage(file)
      await run(() => setMachinePhoto(id, data), 'Foto actualizada')
    } catch (e) {
      addToast((e as Error).message, 'error')
    }
  }

  const remove = async () => {
    if (!m || !confirm(`¿Eliminar ${m.name} con todas sus reservas y fallas?`)) return
    try {
      await deleteMachine(id)
      addToast('Maquina eliminada', 'success')
      navigate('/machines')
    } catch (e) {
      addToast((e as Error).message, 'error')
    }
  }

  const showQr = async () => {
    try { setQr(await fetchMachineQr(id)) } catch (e) { addToast((e as Error).message, 'error') }
  }

  if (missing) {
    return (
      <div className="p-6 text-center">
        <p className="mb-3" style={{ color: 'var(--text-secondary)' }}>Esta maquina no existe o fue eliminada.</p>
        <Link to="/machines" className="text-sm" style={{ color: 'var(--accent)' }}>Ver todas las maquinas</Link>
      </div>
    )
  }
  if (!m) {
    return <div className="flex justify-center py-16"><Loader2 size={24} className="animate-spin" style={{ color: 'var(--accent)' }} /></div>
  }

  const k = kindInfo(m.kind)
  const unusable = m.status !== 'disponible'
  const resolved = m.faults.filter((f) => f.resolved_at)
  const isPast = day < localDate(new Date())
  const certified = !m.requires_certification || staff || certs.some((c) => c.username === user?.username && c.status === 'vigente')

  return (
    <div className="p-4 md:p-6 max-w-4xl mx-auto space-y-4">
      <Link to="/machines" className="inline-flex items-center gap-1 text-sm" style={{ color: 'var(--text-secondary)' }}>
        <ArrowLeft size={14} /> Maquinas
      </Link>

      {/* Ficha */}
      <div className="rounded-xl overflow-hidden md:flex" style={card}>
        <div className="relative md:w-64 h-48 md:h-auto shrink-0 flex items-center justify-center text-6xl" style={{ backgroundColor: 'var(--bg-tertiary, var(--bg-secondary))' }}>
          {m.photo_id ? <img src={machinePhotoUrl(m.photo_id)} alt={m.name} className="w-full h-full object-cover" /> : <span aria-hidden>{k.emoji}</span>}
          {staff && (
            <>
              <button onClick={() => photoInput.current?.click()} className="absolute bottom-2 right-2 p-2 rounded-full" title="Cambiar foto"
                style={{ backgroundColor: 'rgba(0,0,0,.55)', color: '#fff' }}>
                <Camera size={16} />
              </button>
              <input ref={photoInput} type="file" accept="image/*" className="hidden" onChange={(e) => { onPhoto(e.target.files?.[0]); e.target.value = '' }} />
            </>
          )}
        </div>
        <div className="p-5 flex-1 min-w-0 space-y-2">
          <div className="flex items-start justify-between gap-2 flex-wrap">
            <h1 className="text-xl font-semibold" style={{ color: 'var(--text-primary)' }}>{m.name}</h1>
            <MachineStatusChip machine={m} />
          </div>
          <div className="flex gap-4 text-sm flex-wrap" style={{ color: 'var(--text-secondary)' }}>
            <span>{k.emoji} {k.label}</span>
            {m.location && <span className="flex items-center gap-1"><MapPin size={13} /> {m.location}</span>}
            {m.responsible && <span className="flex items-center gap-1"><User size={13} /> Responsable: {m.responsible}</span>}
            {m.max_hours && <span>Max. {m.max_hours} h por reserva</span>}
            {m.requires_approval && <span className="flex items-center gap-1"><Lock size={13} /> Requiere aprobacion</span>}
            {m.requires_certification && <span>🎓 Requiere induccion</span>}
          </div>
          {m.description && <p className="text-sm whitespace-pre-wrap" style={{ color: 'var(--text-primary)' }}>{m.description}</p>}
          <div className="flex gap-2 pt-1 flex-wrap">
            {m.status !== 'retirada' && (
              <button onClick={() => setReporting(true)} className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-sm"
                style={{ border: '1px solid var(--danger)', color: 'var(--danger)' }}>
                <AlertTriangle size={14} /> Reportar falla
              </button>
            )}
            {staff && (
              <>
                <button onClick={() => setEditing(true)} className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-sm" style={{ border: '1px solid var(--border)', color: 'var(--text-primary)' }}>
                  <Pencil size={14} /> Editar
                </button>
                <button onClick={showQr} className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-sm" style={{ border: '1px solid var(--border)', color: 'var(--text-primary)' }}>
                  <QrCode size={14} /> Etiqueta QR
                </button>
              </>
            )}
            {isAdmin && (
              <button onClick={remove} className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-sm" style={{ color: 'var(--danger)' }}>
                <Trash2 size={14} /> Eliminar
              </button>
            )}
          </div>
        </div>
      </div>

      {/* Fallas abiertas */}
      {m.open_faults.map((f) => <OpenFault key={f.id} f={f} staff={staff} onResolve={(note) => run(() => resolveFault(f.id, note), 'Falla cerrada')} />)}

      {/* Instrucciones */}
      {m.instructions && (
        <div className="rounded-xl p-4" style={card}>
          <h2 className="text-sm font-semibold mb-2" style={{ color: 'var(--text-primary)' }}>Instrucciones de uso y seguridad</h2>
          <p className="text-sm whitespace-pre-wrap" style={{ color: 'var(--text-primary)' }}>{m.instructions}</p>
        </div>
      )}

      <InductionPanel machine={m} certs={certs} me={user?.username ?? ''} staff={staff} onChange={load} />

      {/* Reservas del dia */}
      <div className="rounded-xl p-4 space-y-3" style={card}>
        <div className="flex items-center justify-between gap-2">
          <h2 className="text-sm font-semibold" style={{ color: 'var(--text-primary)' }}>Reservas</h2>
          <div className="flex items-center gap-1">
            <button onClick={() => setDay(addDays(day, -1))} className="p-1.5 rounded" style={{ color: 'var(--text-secondary)' }} aria-label="Dia anterior"><ChevronLeft size={16} /></button>
            <input type="date" value={day} onChange={(e) => e.target.value && setDay(e.target.value)} className="px-2 py-1 rounded-lg text-sm outline-none" style={inputStyle} />
            <button onClick={() => setDay(addDays(day, 1))} className="p-1.5 rounded" style={{ color: 'var(--text-secondary)' }} aria-label="Dia siguiente"><ChevronRight size={16} /></button>
          </div>
        </div>
        <div className="text-xs capitalize" style={{ color: 'var(--text-secondary)' }}>{fmtDay(day)}</div>

        <DayTimeline bookings={bookings} me={user?.username ?? ''} />

        {bookings.length === 0 ? (
          <p className="text-sm" style={{ color: 'var(--text-secondary)' }}>Libre todo el dia.</p>
        ) : (
          <div className="space-y-1.5">
            {bookings.map((b) => {
              const own = b.username === user?.username
              return (
                <div key={b.id} className="flex items-center gap-2 text-sm px-3 py-2 rounded-lg flex-wrap" style={{ backgroundColor: 'var(--bg-secondary)' }}>
                  <span className="font-mono text-xs" style={{ color: 'var(--text-primary)' }}>{hhmm(b.start)}–{hhmm(b.end)}</span>
                  <span className="flex-1 min-w-0 truncate" style={{ color: 'var(--text-primary)' }}>
                    {b.kind === 'bloqueo' ? <><Wrench size={12} className="inline" /> Bloqueado</> : b.username}
                    {b.purpose && <span style={{ color: 'var(--text-secondary)' }}> · {b.purpose}</span>}
                  </span>
                  {b.status === 'pendiente' && <span className="text-[11px]" style={{ color: 'var(--warning, #d97706)' }}>por aprobar</span>}
                  {staff && b.status === 'pendiente' && (
                    <>
                      <button onClick={() => run(() => bookingAction(b.id, 'approve'), 'Reserva aprobada')} className="p-1" style={{ color: 'var(--success)' }} title="Aprobar"><Check size={15} /></button>
                      <button onClick={() => run(() => bookingAction(b.id, 'reject'), 'Reserva rechazada')} className="p-1" style={{ color: 'var(--danger)' }} title="Rechazar"><X size={15} /></button>
                    </>
                  )}
                  {(own || staff) && b.status !== 'pendiente' && (
                    <button onClick={() => run(() => bookingAction(b.id, 'cancel'), own ? 'Reserva cancelada' : 'Quitado')} className="text-xs px-2 py-0.5 rounded"
                      style={{ color: 'var(--danger)' }}>{b.kind === 'bloqueo' ? 'Quitar' : 'Cancelar'}</button>
                  )}
                  {own && b.status === 'pendiente' && (
                    <button onClick={() => run(() => bookingAction(b.id, 'cancel'), 'Reserva cancelada')} className="text-xs px-2 py-0.5 rounded" style={{ color: 'var(--danger)' }}>Cancelar</button>
                  )}
                </div>
              )
            })}
          </div>
        )}

        {/* Formulario */}
        {isPast ? null : unusable && !staff ? (
          <p className="text-sm" style={{ color: 'var(--danger)' }}>
            {m.status === 'retirada' ? 'Esta maquina fue retirada.' : 'No se puede reservar mientras este fuera de servicio.'}
          </p>
        ) : !certified ? (
          <p className="text-sm" style={{ color: 'var(--text-secondary)' }}>🎓 Necesitas la induccion para reservar esta maquina (mas arriba puedes pedirla).</p>
        ) : (
          <div className="pt-2 space-y-2" style={{ borderTop: '1px solid var(--border)' }}>
            <div className="flex items-end gap-2 flex-wrap">
              <label className="text-xs" style={{ color: 'var(--text-secondary)' }}>Desde
                <input type="time" value={from} step={900} onChange={(e) => setFrom(e.target.value)} className="block mt-1 px-2 py-1.5 rounded-lg text-sm outline-none" style={inputStyle} />
              </label>
              <label className="text-xs" style={{ color: 'var(--text-secondary)' }}>Hasta
                <input type="time" value={to} step={900} onChange={(e) => setTo(e.target.value)} className="block mt-1 px-2 py-1.5 rounded-lg text-sm outline-none" style={inputStyle} />
              </label>
              <label className="text-xs flex-1 min-w-[10rem]" style={{ color: 'var(--text-secondary)' }}>{block ? 'Motivo' : 'Para que'}
                <input value={purpose} onChange={(e) => setPurpose(e.target.value)} maxLength={500} placeholder={block ? 'Mantencion' : 'Cortar piezas del proyecto X'}
                  className="block w-full mt-1 px-3 py-1.5 rounded-lg text-sm outline-none" style={inputStyle} />
              </label>
              <button onClick={book} disabled={busy || !from || !to} className="flex items-center gap-1.5 px-4 py-2 rounded-lg text-sm font-medium"
                style={{ backgroundColor: 'var(--accent)', color: '#fff', opacity: busy ? 0.7 : 1 }}>
                {busy && <Loader2 size={14} className="animate-spin" />} {block ? 'Bloquear' : 'Reservar'}
              </button>
            </div>
            {staff && (
              <label className="flex items-center gap-2 text-xs cursor-pointer" style={{ color: 'var(--text-secondary)' }}>
                <input type="checkbox" checked={block} onChange={(e) => setBlock(e.target.checked)} />
                Bloquear este horario (mantencion, clase...) en vez de reservar
              </label>
            )}
          </div>
        )}
      </div>

      {/* Historial de fallas */}
      {resolved.length > 0 && (
        <details className="rounded-xl p-4" style={card}>
          <summary className="text-sm font-semibold cursor-pointer" style={{ color: 'var(--text-primary)' }}>Historial de fallas ({resolved.length})</summary>
          <div className="mt-3 space-y-2">
            {resolved.map((f) => (
              <div key={f.id} className="text-sm px-3 py-2 rounded-lg" style={{ backgroundColor: 'var(--bg-secondary)' }}>
                <div style={{ color: 'var(--text-primary)' }}>{f.description}</div>
                <div className="text-xs mt-0.5" style={{ color: 'var(--text-secondary)' }}>
                  {f.reported_by} · {fmtStamp(f.created_at)} → reparada por {f.resolved_by} el {fmtStamp(f.resolved_at ?? '')}
                  {f.resolution_note && `: ${f.resolution_note}`}
                  {f.photo_id && <> · <a href={machinePhotoUrl(f.photo_id)} target="_blank" rel="noreferrer" style={{ color: 'var(--accent)' }}>foto</a></>}
                </div>
              </div>
            ))}
          </div>
        </details>
      )}

      {editing && <MachineFormModal machine={m} onClose={() => setEditing(false)} onSaved={() => { setEditing(false); load() }} />}
      {reporting && <FaultModal name={m.name} onClose={() => setReporting(false)} onSend={async (d, p) => {
        const ok = await run(() => reportFault(id, d, p), 'Falla reportada: avisamos a los operadores')
        if (ok) setReporting(false)
      }} />}
      {qr && <QrModal name={m.name} location={m.location} qr={qr} onClose={() => setQr(null)} />}
    </div>
  )
}

/** Barra horizontal del dia con los horarios ocupados */
function DayTimeline({ bookings, me }: { bookings: MachineBooking[]; me: string }) {
  const span = (DAY_END - DAY_START) * 60
  const pos = (s: string) => {
    const mins = Number(s.slice(11, 13)) * 60 + Number(s.slice(14, 16)) - DAY_START * 60
    return Math.max(0, Math.min(span, mins)) / span * 100
  }
  const hours = Array.from({ length: DAY_END - DAY_START + 1 }, (_, i) => DAY_START + i)
  return (
    <div>
      <div className="relative h-7 rounded-md overflow-hidden" style={{ backgroundColor: 'var(--bg-secondary)' }}>
        {bookings.map((b) => {
          const left = pos(b.start)
          // si termina al dia siguiente, hasta el borde
          const right = b.end.slice(0, 10) > b.start.slice(0, 10) ? 100 : pos(b.end)
          const color = b.kind === 'bloqueo' ? 'var(--text-secondary)' : b.username === me ? 'var(--accent)' : 'var(--warning, #d97706)'
          return (
            <div key={b.id} className="absolute top-0 bottom-0" title={`${hhmm(b.start)}–${hhmm(b.end)} ${b.kind === 'bloqueo' ? 'bloqueado' : b.username}`}
              style={{ left: `${left}%`, width: `${Math.max(right - left, 0.8)}%`, backgroundColor: color, opacity: b.status === 'pendiente' ? 0.45 : 0.85 }} />
          )
        })}
      </div>
      <div className="relative h-4 mt-0.5 text-[10px]" style={{ color: 'var(--text-secondary)' }}>
        {hours.filter((h) => h % 3 === 1 || h === DAY_START).map((h) => (
          <span key={h} className="absolute -translate-x-1/2" style={{ left: `${(h - DAY_START) / (DAY_END - DAY_START) * 100}%` }}>{h}</span>
        ))}
      </div>
    </div>
  )
}

function OpenFault({ f, staff, onResolve }: { f: MachineFault; staff: boolean; onResolve: (note: string) => Promise<boolean> }) {
  const [note, setNote] = useState('')
  return (
    <div className="rounded-xl p-4 space-y-2" style={{ border: '1px solid var(--danger)', backgroundColor: 'color-mix(in srgb, var(--danger) 8%, transparent)' }}>
      <div className="flex items-start gap-3">
        <AlertTriangle size={18} className="shrink-0 mt-0.5" style={{ color: 'var(--danger)' }} />
        <div className="flex-1 min-w-0">
          <div className="text-sm font-medium" style={{ color: 'var(--text-primary)' }}>{f.description}</div>
          <div className="text-xs" style={{ color: 'var(--text-secondary)' }}>Reportada por {f.reported_by} · {fmtStamp(f.created_at)}</div>
        </div>
        {f.photo_id && (
          <a href={machinePhotoUrl(f.photo_id)} target="_blank" rel="noreferrer" className="shrink-0">
            <img src={machinePhotoUrl(f.photo_id)} alt="Foto de la falla" className="w-16 h-16 object-cover rounded-lg" />
          </a>
        )}
      </div>
      {staff && (
        <div className="flex gap-2 flex-wrap">
          <input value={note} onChange={(e) => setNote(e.target.value)} maxLength={2000} placeholder="Que se hizo (cambio de fusible, limpieza...)"
            className="flex-1 min-w-[12rem] px-3 py-1.5 rounded-lg text-sm outline-none" style={inputStyle} />
          <button onClick={async () => { if (await onResolve(note)) setNote('') }} className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-sm font-medium"
            style={{ backgroundColor: 'var(--success)', color: '#fff' }}>
            <Check size={14} /> Marcar reparada
          </button>
        </div>
      )}
    </div>
  )
}

function FaultModal({ name, onClose, onSend }: { name: string; onClose: () => void; onSend: (desc: string, photo?: string) => Promise<void> }) {
  const [desc, setDesc] = useState('')
  const [photo, setPhoto] = useState<string>()
  const [sending, setSending] = useState(false)
  const { addToast } = useToast()
  return (
    <div className="fixed inset-0 z-50 flex items-end sm:items-center justify-center bg-black/50" onClick={onClose}>
      <div className="rounded-t-xl sm:rounded-xl p-5 w-full max-w-md sm:mx-4 space-y-3" style={{ backgroundColor: 'var(--bg-secondary)', border: '1px solid var(--border)' }}
        onClick={(e) => e.stopPropagation()}>
        <div className="flex items-center justify-between">
          <h3 className="font-semibold" style={{ color: 'var(--text-primary)' }}>Falla en {name}</h3>
          <button onClick={onClose} style={{ color: 'var(--text-secondary)' }}><X size={18} /></button>
        </div>
        <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>
          La maquina queda fuera de servicio y se avisa a los operadores por Telegram. Quienes tengan reservas reciben un aviso.
        </p>
        <textarea value={desc} onChange={(e) => setDesc(e.target.value)} rows={4} maxLength={2000} autoFocus placeholder="Que paso: no enciende, ruido raro, lente sucio..."
          className="w-full px-3 py-2 rounded-lg text-sm outline-none resize-y" style={inputStyle} />
        <div className="flex items-center gap-3">
          <label className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-sm cursor-pointer" style={{ border: '1px solid var(--border)', color: 'var(--text-primary)' }}>
            <Camera size={14} /> {photo ? 'Cambiar foto' : 'Agregar foto'}
            <input type="file" accept="image/*" capture="environment" className="hidden" onChange={async (e) => {
              const f = e.target.files?.[0]
              e.target.value = ''
              if (!f) return
              try { setPhoto(await compressImage(f)) } catch (err) { addToast((err as Error).message, 'error') }
            }} />
          </label>
          {photo && <img src={photo} alt="" className="w-12 h-12 object-cover rounded" />}
        </div>
        <div className="flex justify-end gap-2">
          <button onClick={onClose} className="px-4 py-2 text-sm" style={{ color: 'var(--text-secondary)' }}>Cancelar</button>
          <button disabled={!desc.trim() || sending} onClick={async () => { setSending(true); await onSend(desc.trim(), photo); setSending(false) }}
            className="flex items-center gap-1.5 px-4 py-2 rounded-lg text-sm font-medium"
            style={{ backgroundColor: 'var(--danger)', color: '#fff', opacity: !desc.trim() || sending ? 0.6 : 1 }}>
            {sending && <Loader2 size={14} className="animate-spin" />} Reportar
          </button>
        </div>
      </div>
    </div>
  )
}

/** Etiqueta para imprimir y pegar en la maquina */
function QrModal({ name, location, qr, onClose }: { name: string; location: string; qr: { url: string; svg: string }; onClose: () => void }) {
  const print = () => {
    const w = window.open('', '_blank', 'width=420,height=560')
    if (!w) return
    const esc = (s: string) => s.replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]!))
    w.document.write(`<!doctype html><html><head><meta charset="utf-8"><title>${esc(name)}</title>
      <style>body{font-family:system-ui,sans-serif;text-align:center;margin:0;padding:24px}
      .l{display:inline-block;border:2px dashed #999;border-radius:12px;padding:16px 20px}
      svg{width:220px;height:220px}h1{font-size:20px;margin:8px 0 2px}p{margin:0;color:#555;font-size:13px}
      @media print{.l{border-color:#ccc}}</style></head>
      <body><div class="l">${qr.svg}<h1>${esc(name)}</h1>${location ? `<p>${esc(location)}</p>` : ''}
      <p style="margin-top:6px">Escanea para reservar o reportar una falla</p></div>
      <script>window.onload=function(){window.print()}</script></body></html>`)
    w.document.close()
  }
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50" onClick={onClose}>
      <div className="rounded-xl p-5 w-full max-w-sm mx-4 text-center space-y-3" style={{ backgroundColor: 'var(--bg-secondary)', border: '1px solid var(--border)' }}
        onClick={(e) => e.stopPropagation()}>
        <div className="mx-auto w-56 h-56 bg-white rounded-lg p-2 [&>svg]:w-full [&>svg]:h-full" dangerouslySetInnerHTML={{ __html: qr.svg }} />
        <div className="font-semibold" style={{ color: 'var(--text-primary)' }}>{name}</div>
        <div className="text-[11px] break-all" style={{ color: 'var(--text-secondary)' }}>{qr.url}</div>
        <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>
          El QR usa la direccion con que abriste LabNAS. Si lo abriste con localhost, entra con la IP del servidor antes de imprimir.
        </p>
        <div className="flex justify-center gap-2">
          <button onClick={onClose} className="px-4 py-2 text-sm" style={{ color: 'var(--text-secondary)' }}>Cerrar</button>
          <button onClick={print} className="flex items-center gap-1.5 px-4 py-2 rounded-lg text-sm font-medium" style={{ backgroundColor: 'var(--accent)', color: '#fff' }}>
            <Printer size={14} /> Imprimir etiqueta
          </button>
        </div>
      </div>
    </div>
  )
}

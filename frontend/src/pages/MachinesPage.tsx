import { useCallback, useEffect, useMemo, useState } from 'react'
import { Link, useNavigate } from 'react-router-dom'
import { Check, Loader2, MapPin, Plus, Wrench, X } from 'lucide-react'
import {
  bookingAction,
  fetchAllCertifications,
  fetchBookings,
  grantCertification,
  revokeCertification,
  fetchMachines,
  kindInfo,
  localDateTime,
  MACHINE_KINDS,
  machinePhotoUrl,
  shortDateTime,
  hhmm,
  type Machine,
  type MachineBooking,
  type MachineCertification,
  type MachineKind,
} from '../api'
import { useAuth } from '../auth/AuthContext'
import { useEvent } from '../events/useEvents'
import { useToast } from '../components/ToastContext'
import MachineFormModal from '../components/machines/MachineFormModal'
import MachineStatusChip from '../components/machines/MachineStatusChip'

type Tab = 'machines' | 'mine' | 'pending'

export default function MachinesPage() {
  const { canModule } = useAuth()
  const staff = canModule('machines', 'gestionar')
  const { addToast } = useToast()
  const navigate = useNavigate()
  const [tab, setTab] = useState<Tab>('machines')
  const [machines, setMachines] = useState<Machine[]>([])
  const [mine, setMine] = useState<MachineBooking[]>([])
  const [pending, setPending] = useState<MachineBooking[]>([])
  const [certReqs, setCertReqs] = useState<MachineCertification[]>([])
  const [loading, setLoading] = useState(true)
  const [kind, setKind] = useState<MachineKind | ''>('')
  const [creating, setCreating] = useState(false)

  const load = useCallback(async () => {
    try {
      const now = localDateTime(new Date())
      const [m, b, p, c] = await Promise.all([
        fetchMachines(),
        fetchBookings({ mine: true, from: now }),
        staff ? fetchBookings({ status: 'pendiente', from: now }) : Promise.resolve([]),
        staff ? fetchAllCertifications({ status: 'solicitada' }) : Promise.resolve([]),
      ])
      setMachines(m)
      setMine(b)
      setPending(p)
      setCertReqs(c)
    } catch (e) {
      addToast((e as Error).message, 'error')
    } finally {
      setLoading(false)
    }
  }, [staff, addToast])

  useEffect(() => { load() }, [load])
  useEvent('machines', load, load)

  const act = async (b: MachineBooking, action: 'cancel' | 'approve' | 'reject') => {
    try {
      await bookingAction(b.id, action)
      addToast(action === 'cancel' ? 'Reserva cancelada' : action === 'approve' ? 'Reserva aprobada' : 'Reserva rechazada', 'success')
      load()
    } catch (e) {
      addToast((e as Error).message, 'error')
    }
  }

  const certAct = async (c: MachineCertification, grant: boolean) => {
    try {
      if (grant) await grantCertification(c.machine_id, { username: c.username })
      else await revokeCertification(c.machine_id, c.username)
      addToast(grant ? `Induccion dada a ${c.username}` : 'Solicitud rechazada', 'success')
      load()
    } catch (e) {
      addToast((e as Error).message, 'error')
    }
  }

  const shown = useMemo(() => machines.filter((m) => !kind || m.kind === kind), [machines, kind])
  const kindsPresent = useMemo(() => MACHINE_KINDS.filter((k) => machines.some((m) => m.kind === k.id)), [machines])

  const tabs: { id: Tab; label: string; count?: number }[] = [
    { id: 'machines', label: 'Maquinas' },
    { id: 'mine', label: 'Mis reservas', count: mine.length },
    ...(staff ? [{ id: 'pending' as Tab, label: 'Por aprobar', count: pending.length + certReqs.length }] : []),
  ]

  return (
    <div className="p-4 md:p-6 max-w-6xl mx-auto">
      <div className="flex items-center justify-between gap-3 mb-4 flex-wrap">
        <div className="flex gap-1 p-1 rounded-lg" style={{ backgroundColor: 'var(--bg-secondary)', border: '1px solid var(--border)' }}>
          {tabs.map((t) => (
            <button key={t.id} onClick={() => setTab(t.id)} className="px-3 py-1.5 rounded-md text-sm transition-all"
              style={{ backgroundColor: tab === t.id ? 'var(--accent)' : 'transparent', color: tab === t.id ? '#fff' : 'var(--text-secondary)' }}>
              {t.label}
              {!!t.count && <span className="ml-1.5 text-[10px] px-1.5 rounded-full" style={{ backgroundColor: tab === t.id ? 'rgba(255,255,255,.25)' : 'var(--accent-alpha)' }}>{t.count}</span>}
            </button>
          ))}
        </div>
        {staff && tab === 'machines' && (
          <button onClick={() => setCreating(true)} className="flex items-center gap-2 px-4 py-2 rounded-lg text-sm font-medium"
            style={{ backgroundColor: 'var(--accent)', color: '#fff' }}>
            <Plus size={16} /> Nueva maquina
          </button>
        )}
      </div>

      {loading ? (
        <div className="flex justify-center py-16"><Loader2 size={24} className="animate-spin" style={{ color: 'var(--accent)' }} /></div>
      ) : tab === 'machines' ? (
        <>
          {kindsPresent.length > 1 && (
            <div className="flex gap-2 mb-4 flex-wrap">
              {[{ id: '' as const, label: 'Todas', emoji: '' }, ...kindsPresent].map((k) => (
                <button key={k.id} onClick={() => setKind(k.id)} className="px-3 py-1 rounded-full text-xs"
                  style={{ border: '1px solid var(--border)', backgroundColor: kind === k.id ? 'var(--accent-alpha)' : 'transparent', color: kind === k.id ? 'var(--accent)' : 'var(--text-secondary)' }}>
                  {k.emoji} {k.label}
                </button>
              ))}
            </div>
          )}
          {shown.length === 0 ? (
            <div className="text-center py-16">
              <Wrench size={48} className="mx-auto mb-3" style={{ color: 'var(--text-secondary)', opacity: 0.3 }} />
              <p className="text-sm" style={{ color: 'var(--text-secondary)' }}>
                {machines.length === 0
                  ? staff ? 'Agrega las maquinas del laboratorio para que el equipo pueda reservarlas' : 'Todavia no hay maquinas registradas'
                  : 'Sin maquinas de este tipo'}
              </p>
            </div>
          ) : (
            <div className="grid gap-4 grid-cols-1 sm:grid-cols-2 lg:grid-cols-3">
              {shown.map((m) => <MachineCard key={m.id} m={m} />)}
            </div>
          )}
        </>
      ) : tab === 'mine' ? (
        <BookingList list={mine} empty="No tienes reservas proximas" actions={(b) => [{ label: 'Cancelar', icon: X, danger: true, run: () => act(b, 'cancel') }]} />
      ) : (
        <div className="space-y-6">
          {(pending.length > 0 || certReqs.length === 0) && (
            <BookingList list={pending} empty="No hay reservas ni inducciones esperando aprobacion" showUser actions={(b) => [
              { label: 'Aprobar', icon: Check, run: () => act(b, 'approve') },
              { label: 'Rechazar', icon: X, danger: true, run: () => act(b, 'reject') },
            ]} />
          )}
          {certReqs.length > 0 && (
            <div className="space-y-2">
              <h3 className="text-sm font-semibold" style={{ color: 'var(--text-primary)' }}>🎓 Inducciones pedidas</h3>
              {certReqs.map((c) => (
                <div key={`${c.machine_id}:${c.username}`} className="flex items-center gap-3 px-4 py-3 rounded-xl flex-wrap"
                  style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}>
                  <div className="flex-1 min-w-[12rem]">
                    <div className="text-sm" style={{ color: 'var(--text-primary)' }}>
                      <span className="font-medium">{c.username}</span> pide la induccion de{' '}
                      <Link to={`/machines/${c.machine_id}`} className="font-medium hover:underline">{c.machine_name}</Link>
                    </div>
                    {c.notes && <div className="text-xs" style={{ color: 'var(--text-secondary)' }}>{c.notes}</div>}
                  </div>
                  <div className="flex gap-2">
                    <button onClick={() => certAct(c, true)} className="flex items-center gap-1 px-3 py-1.5 rounded-lg text-xs"
                      style={{ border: '1px solid var(--border)', color: 'var(--success)' }}><Check size={13} /> Ya la hizo</button>
                    <button onClick={() => certAct(c, false)} className="flex items-center gap-1 px-3 py-1.5 rounded-lg text-xs"
                      style={{ border: '1px solid var(--border)', color: 'var(--danger)' }}><X size={13} /> Rechazar</button>
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>
      )}

      {creating && (
        <MachineFormModal onClose={() => setCreating(false)} onSaved={(m) => { setCreating(false); navigate(`/machines/${m.id}`) }} />
      )}
    </div>
  )
}

function MachineCard({ m }: { m: Machine }) {
  const k = kindInfo(m.kind)
  return (
    <Link to={`/machines/${m.id}`} className="rounded-xl overflow-hidden transition-all hover:-translate-y-0.5 block"
      style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)', opacity: m.status === 'retirada' ? 0.6 : 1 }}>
      <div className="h-32 flex items-center justify-center text-5xl" style={{ backgroundColor: 'var(--bg-tertiary, var(--bg-secondary))' }}>
        {m.photo_id
          ? <img src={machinePhotoUrl(m.photo_id)} alt={m.name} className="w-full h-full object-cover" loading="lazy" />
          : <span aria-hidden>{k.emoji}</span>}
      </div>
      <div className="p-4 space-y-1.5">
        <div className="flex items-start justify-between gap-2">
          <span className="font-medium truncate" style={{ color: 'var(--text-primary)' }}>{m.name}</span>
          <MachineStatusChip machine={m} />
        </div>
        <div className="flex items-center gap-3 text-xs" style={{ color: 'var(--text-secondary)' }}>
          <span>{k.label}</span>
          {m.location && <span className="flex items-center gap-0.5 truncate"><MapPin size={11} /> {m.location}</span>}
        </div>
        {m.next && m.status === 'disponible' && (
          <div className="text-xs" style={{ color: 'var(--text-secondary)' }}>
            Proxima: {shortDateTime(m.next.start)}–{hhmm(m.next.end)}{m.next.kind === 'reserva' ? ` · ${m.next.username}` : ' · bloqueo'}
          </div>
        )}
        {m.requires_certification && (
          <div className="text-xs" style={{ color: 'var(--text-secondary)' }}>🎓 Requiere induccion</div>
        )}
        {m.open_faults.length > 0 && (
          <div className="text-xs truncate" style={{ color: 'var(--danger)' }}>🔧 {m.open_faults[0].description}</div>
        )}
      </div>
    </Link>
  )
}

interface BookingAction {
  label: string
  icon: typeof X
  danger?: boolean
  run: () => void
}

function BookingList({ list, empty, showUser, actions }: {
  list: MachineBooking[]
  empty: string
  showUser?: boolean
  actions: (b: MachineBooking) => BookingAction[]
}) {
  if (list.length === 0) {
    return <p className="text-center py-16 text-sm" style={{ color: 'var(--text-secondary)' }}>{empty}</p>
  }
  return (
    <div className="space-y-2">
      {list.map((b) => (
        <div key={b.id} className="flex items-center gap-3 px-4 py-3 rounded-xl flex-wrap"
          style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}>
          <div className="flex-1 min-w-[12rem]">
            <Link to={`/machines/${b.machine_id}`} className="text-sm font-medium hover:underline" style={{ color: 'var(--text-primary)' }}>
              {b.machine_name}
            </Link>
            <div className="text-xs" style={{ color: 'var(--text-secondary)' }}>
              {shortDateTime(b.start)}–{hhmm(b.end)}
              {showUser && ` · ${b.username}`}
              {b.purpose && ` · ${b.purpose}`}
            </div>
          </div>
          {b.status === 'pendiente' && !showUser && (
            <span className="text-[11px] px-2 py-0.5 rounded-full" style={{ color: 'var(--warning, #d97706)', border: '1px solid currentColor' }}>Esperando aprobacion</span>
          )}
          <div className="flex gap-2">
            {actions(b).map((a) => (
              <button key={a.label} onClick={a.run} className="flex items-center gap-1 px-3 py-1.5 rounded-lg text-xs"
                style={{ border: '1px solid var(--border)', color: a.danger ? 'var(--danger)' : 'var(--success)' }}>
                <a.icon size={13} /> {a.label}
              </button>
            ))}
          </div>
        </div>
      ))}
    </div>
  )
}

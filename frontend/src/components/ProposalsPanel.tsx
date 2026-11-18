import { useCallback, useEffect, useState } from 'react'
import { CalendarPlus, ClipboardList, Loader2, Pencil, Check, X, Sparkles, MapPin } from 'lucide-react'
import { fetchProposals, approveProposal, dismissProposal, type Proposal, type ProposalEdits } from '../api'
import { useEvent } from '../events/useEvents'
import { useToast } from './ToastContext'
import { errorMessage } from '../lib/errors'

const inputStyle = { backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }

function ProposalCard({ p, onDone }: { p: Proposal; onDone: (id: string) => void }) {
  const { addToast } = useToast()
  const [editing, setEditing] = useState(false)
  const [busy, setBusy] = useState(false)
  const [title, setTitle] = useState(p.title)
  const [date, setDate] = useState(p.date ?? '')
  const [time, setTime] = useState(p.time ?? '')
  const [endTime, setEndTime] = useState(p.end_time ?? '')
  const [location, setLocation] = useState(p.location ?? '')
  const isEvent = p.kind === 'evento'
  const Icon = isEvent ? CalendarPlus : ClipboardList

  async function approve() {
    setBusy(true)
    try {
      let edits: ProposalEdits | undefined
      if (editing) {
        edits = { title: title.trim(), date, time }
        if (isEvent) { edits.end_time = endTime; edits.location = location }
      }
      const r = await approveProposal(p.id, edits)
      addToast(r.message || (isEvent ? 'Evento agendado' : 'Tarea creada'), 'success')
      onDone(p.id)
    } catch (e) {
      addToast(errorMessage(e), 'error')
      setBusy(false)
    }
  }

  async function dismiss() {
    setBusy(true)
    try {
      await dismissProposal(p.id)
      onDone(p.id)
    } catch (e) {
      addToast(errorMessage(e), 'error')
      setBusy(false)
    }
  }

  const when = [p.date, p.time && (p.end_time ? `${p.time} - ${p.end_time}` : p.time)].filter(Boolean).join(' ')

  return (
    <div className="rounded-xl p-4 space-y-3" style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}>
      <div className="flex items-start gap-3">
        <Icon size={20} className="shrink-0 mt-0.5" style={{ color: 'var(--accent)' }} />
        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-2 flex-wrap">
            <span className="text-sm font-medium" style={{ color: 'var(--text-primary)' }}>{p.title}</span>
            <span className="text-[10px] px-1.5 py-0.5 rounded-full font-medium" style={{ backgroundColor: 'var(--bg-tertiary)', color: 'var(--text-secondary)' }}>
              {isEvent ? 'Evento' : 'Tarea'}
            </span>
            {p.source === 'ics' && (
              <span className="text-[10px] px-1.5 py-0.5 rounded-full font-medium" style={{ backgroundColor: 'var(--success)' + '25', color: 'var(--success)' }}>
                Invitación .ics
              </span>
            )}
          </div>
          {(when || p.location) && (
            <p className="text-xs mt-1 flex items-center gap-3 flex-wrap" style={{ color: 'var(--text-secondary)' }}>
              {when && <span>{when}</span>}
              {p.location && <span className="flex items-center gap-1"><MapPin size={11} />{p.location}</span>}
            </p>
          )}
          {p.description && <p className="text-xs mt-1" style={{ color: 'var(--text-secondary)' }}>{p.description}</p>}
          {(p.email_subject || p.email_from) && (
            <p className="text-[11px] mt-1 truncate" style={{ color: 'var(--text-secondary)', opacity: 0.8 }}>
              Del correo: {p.email_subject}{p.email_from ? ` - ${p.email_from.split('<')[0].trim() || p.email_from}` : ''}
            </p>
          )}
        </div>
      </div>

      {editing && (
        <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
          <div className="sm:col-span-2">
            <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Título</label>
            <input value={title} onChange={e => setTitle(e.target.value)} className="w-full px-3 py-2 rounded-lg text-sm outline-none" style={inputStyle} />
          </div>
          <div>
            <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Fecha</label>
            <input type="date" value={date} onChange={e => setDate(e.target.value)} className="w-full px-3 py-2 rounded-lg text-sm outline-none" style={inputStyle} />
          </div>
          <div>
            <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Hora</label>
            <input type="time" value={time} onChange={e => setTime(e.target.value)} className="w-full px-3 py-2 rounded-lg text-sm outline-none" style={inputStyle} />
          </div>
          {isEvent && (
            <>
              <div>
                <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Hora de término</label>
                <input type="time" value={endTime} onChange={e => setEndTime(e.target.value)} className="w-full px-3 py-2 rounded-lg text-sm outline-none" style={inputStyle} />
              </div>
              <div>
                <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Lugar</label>
                <input value={location} onChange={e => setLocation(e.target.value)} className="w-full px-3 py-2 rounded-lg text-sm outline-none" style={inputStyle} />
              </div>
            </>
          )}
        </div>
      )}

      <div className="flex items-center gap-2 flex-wrap">
        <button onClick={approve} disabled={busy || (editing && !title.trim())}
          className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium transition-all hover:opacity-90"
          style={{ backgroundColor: 'var(--success)', color: '#fff' }}>
          {busy ? <Loader2 size={14} className="animate-spin" /> : <Check size={14} />}
          Aprobar
        </button>
        <button onClick={() => setEditing(v => !v)} disabled={busy}
          className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium transition-all hover:opacity-90"
          style={{ color: 'var(--text-primary)', border: '1px solid var(--border)' }}>
          <Pencil size={14} />
          {editing ? 'Cerrar edición' : 'Editar'}
        </button>
        <button onClick={dismiss} disabled={busy}
          className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium transition-all hover:opacity-90"
          style={{ color: 'var(--danger)', border: '1px solid var(--danger)' }}>
          <X size={14} />
          Descartar
        </button>
      </div>
    </div>
  )
}

export default function ProposalsPanel() {
  const [items, setItems] = useState<Proposal[]>([])

  const load = useCallback(async () => {
    try { setItems(await fetchProposals('pendiente')) } catch { /* sin cuenta o sin IA */ }
  }, [])

  useEffect(() => {
    // eslint-disable-next-line react-hooks/set-state-in-effect
    void load()
  }, [load])
  useEvent('email.proposals', () => { void load() }, () => { void load() })

  if (items.length === 0) return null

  return (
    <section className="space-y-3">
      <div className="flex items-center gap-2">
        <Sparkles size={18} style={{ color: 'var(--accent)' }} />
        <h2 className="text-base font-semibold" style={{ color: 'var(--text-primary)' }}>Propuestas de la IA</h2>
        <span className="text-xs" style={{ color: 'var(--text-secondary)' }}>
          {items.length} pendiente{items.length !== 1 ? 's' : ''} - nada se agenda sin tu aprobación
        </span>
      </div>
      {items.map(p => (
        <ProposalCard key={p.id} p={p} onDone={id => setItems(prev => prev.filter(x => x.id !== id))} />
      ))}
    </section>
  )
}

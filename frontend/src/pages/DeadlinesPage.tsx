import { useCallback, useEffect, useMemo, useState } from 'react'
import { History, Hourglass, Loader2, Plus } from 'lucide-react'
import {
  categoryInfo,
  deleteDeadline,
  fetchDeadlines,
  setDeadlineDone,
  type Deadline,
} from '../api'
import { useAuth } from '../auth/AuthContext'
import { useEvent } from '../events/useEvents'
import { useToast } from '../components/ToastContext'
import DeadlineCard from '../components/deadlines/DeadlineCard'
import AccessBadge from '../components/access/AccessBadge'
import { useAccessLevels } from '../components/access/useAccessLevels'
import DeadlineFormModal from '../components/deadlines/DeadlineFormModal'

type Filter = 'todos' | 'urgentes' | string

export default function DeadlinesPage() {
  const { user, canModule } = useAuth()
  const staff = canModule('deadlines', 'gestionar')
  const { addToast } = useToast()
  const [list, setList] = useState<Deadline[]>([])
  const [done, setDone] = useState<Deadline[]>([])
  const [showDone, setShowDone] = useState(false)
  const [loading, setLoading] = useState(true)
  const [filter, setFilter] = useState<Filter>('todos')
  const [editing, setEditing] = useState<Deadline | null | 'new'>(null)

  const load = useCallback(async () => {
    try {
      const [a, b] = await Promise.all([fetchDeadlines(), showDone ? fetchDeadlines(true) : Promise.resolve([])])
      setList(a)
      setDone(b)
    } catch (e) {
      addToast((e as Error).message, 'error')
    } finally {
      setLoading(false)
    }
  }, [showDone, addToast])

  useEffect(() => { load() }, [load])
  useEvent('deadlines', load, load)

  const canEdit = (d: Deadline) => d.source === 'manual' && (staff || d.created_by === user?.username)
  // Nivel de acceso: solo los contadores sueltos (los del Portafolio siguen al proyecto)
  const access = useAccessLevels('plazo', [...list, ...done].flatMap(d => (d.id ? [d.id] : [])))
  const accessFor = (d: Deadline) => d.id
    ? <AccessBadge kind="plazo" id={d.id} level={access.level(d.id)} canManage={canEdit(d)} compact onChanged={v => access.set(d.id!, v)} />
    : undefined

  const markDone = async (d: Deadline, value: boolean) => {
    if (!d.id) return
    try {
      await setDeadlineDone(d.id, value)
      addToast(value ? 'Plazo cumplido' : 'Plazo reabierto', 'success')
      load()
    } catch (e) {
      addToast((e as Error).message, 'error')
    }
  }

  const remove = async (d: Deadline) => {
    if (!d.id || !confirm(`¿Eliminar el plazo «${d.title}»?`)) return
    try {
      await deleteDeadline(d.id)
      addToast('Plazo eliminado', 'success')
      load()
    } catch (e) {
      addToast((e as Error).message, 'error')
    }
  }

  const counts = useMemo(() => ({
    vencido: list.filter((d) => d.level === 'vencido').length,
    urgente: list.filter((d) => d.level === 'urgente').length,
    pronto: list.filter((d) => d.level === 'pronto').length,
  }), [list])

  const categories = useMemo(() => {
    const ids = [...new Set(list.map((d) => d.category))]
    return ids.map((id) => categoryInfo(id))
  }, [list])

  const shown = useMemo(() => list.filter((d) =>
    filter === 'todos' ? true
      : filter === 'urgentes' ? d.level === 'vencido' || d.level === 'urgente'
        : d.category === filter,
  ), [list, filter])

  const chip = (id: Filter, label: string) => (
    <button key={id} onClick={() => setFilter(id)} className="px-3 py-1 rounded-full text-xs"
      style={{ border: '1px solid var(--border)', backgroundColor: filter === id ? 'var(--accent-alpha)' : 'transparent', color: filter === id ? 'var(--accent)' : 'var(--text-secondary)' }}>
      {label}
    </button>
  )

  return (
    <div className="p-4 md:p-6 max-w-5xl mx-auto">
      <div className="flex items-center justify-between gap-3 mb-4 flex-wrap">
        <div className="flex gap-4 text-sm" style={{ color: 'var(--text-secondary)' }}>
          <Stat n={counts.vencido} label="vencidos" color="var(--danger)" />
          <Stat n={counts.urgente} label="en 7 dias" color="var(--danger)" />
          <Stat n={counts.pronto} label="en 30 dias" color="var(--warning, #d97706)" />
        </div>
        <button onClick={() => setEditing('new')} className="flex items-center gap-2 px-4 py-2 rounded-lg text-sm font-medium"
          style={{ backgroundColor: 'var(--accent)', color: '#fff' }}>
          <Plus size={16} /> Nuevo plazo
        </button>
      </div>

      {list.length > 0 && (
        <div className="flex gap-2 mb-4 flex-wrap">
          {chip('todos', 'Todos')}
          {(counts.vencido + counts.urgente > 0) && chip('urgentes', '🔴 Urgentes')}
          {categories.length > 1 && categories.map((c) => chip(c.id, `${c.emoji} ${c.label}`))}
        </div>
      )}

      {loading ? (
        <div className="flex justify-center py-16"><Loader2 size={24} className="animate-spin" style={{ color: 'var(--accent)' }} /></div>
      ) : shown.length === 0 ? (
        <div className="text-center py-16">
          <Hourglass size={48} className="mx-auto mb-3" style={{ color: 'var(--text-secondary)', opacity: 0.3 }} />
          <p className="text-sm max-w-md mx-auto" style={{ color: 'var(--text-secondary)' }}>
            {list.length === 0
              ? 'Sin plazos. Crea uno (informe, rendición, garantía, convocatoria...) o pon fecha de término e hitos a los proyectos del Portafolio y aparecerán aquí.'
              : 'Nada con este filtro'}
          </p>
        </div>
      ) : (
        <div className="grid gap-3 grid-cols-1 md:grid-cols-2">
          {shown.map((d) => (
            <DeadlineCard key={d.key} d={d} editable={canEdit(d)} onEdit={setEditing} onDone={markDone} onDelete={remove} access={accessFor(d)} />
          ))}
        </div>
      )}

      <div className="mt-8">
        <button onClick={() => setShowDone((v) => !v)} className="flex items-center gap-2 text-sm" style={{ color: 'var(--text-secondary)' }}>
          <History size={15} /> {showDone ? 'Ocultar cumplidos' : 'Ver cumplidos'}
        </button>
        {showDone && (
          done.length === 0
            ? <p className="text-xs mt-3" style={{ color: 'var(--text-secondary)' }}>Todavía no hay plazos cumplidos</p>
            : (
              <div className="grid gap-3 grid-cols-1 md:grid-cols-2 mt-3 opacity-80">
                {done.map((d) => (
                  <DeadlineCard key={d.key} d={d} editable={canEdit(d)} onDone={markDone} onDelete={remove} access={accessFor(d)} />
                ))}
              </div>
            )
        )}
      </div>

      {editing && (
        <DeadlineFormModal
          deadline={editing === 'new' ? null : editing}
          onClose={() => setEditing(null)}
          onSaved={() => { setEditing(null); load() }}
        />
      )}
    </div>
  )
}

function Stat({ n, label, color }: { n: number; label: string; color: string }) {
  return (
    <span className="flex items-baseline gap-1">
      <span className="text-xl font-bold" style={{ color: n ? color : 'var(--text-secondary)' }}>{n}</span> {label}
    </span>
  )
}

import { useCallback, useEffect, useState } from 'react'
import { Link } from 'react-router-dom'
import { Hourglass } from 'lucide-react'
import { fetchDeadlines, type Deadline } from '../../api'
import { useEvent } from '../../events/useEvents'
import DeadlineCard from './DeadlineCard'

/** Dashboard: los plazos vencidos y los de los proximos 60 dias (maximo 6) */
export default function DeadlinesWidget() {
  const [list, setList] = useState<Deadline[] | null>(null)

  const load = useCallback(() => {
    fetchDeadlines()
      .then((all) => setList(all.filter((d) => d.days_left <= 60).slice(0, 6)))
      .catch(() => setList([]))
  }, [])

  useEffect(() => { load() }, [load])
  useEvent('deadlines', load, load)

  if (!list || list.length === 0) return null

  return (
    <div className="rounded-xl p-6" style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}>
      <div className="flex items-center gap-3 mb-4">
        <Hourglass size={22} style={{ color: 'var(--accent)' }} />
        <h2 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>Plazos</h2>
        <Link to="/deadlines" className="ml-auto text-xs hover:underline" style={{ color: 'var(--accent)' }}>Ver todos</Link>
      </div>
      <div className="grid gap-3 grid-cols-1 md:grid-cols-2 lg:grid-cols-3">
        {list.map((d) => <DeadlineCard key={d.key} d={d} compact />)}
      </div>
    </div>
  )
}

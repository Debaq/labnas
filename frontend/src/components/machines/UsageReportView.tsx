import { useCallback, useEffect, useState } from 'react'
import { Loader2 } from 'lucide-react'
import { fetchUsageReport, localDate, type UsageReport, type UsageReportRow } from '../../api'
import { useToast } from '../ToastContext'

const inputStyle = { backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }
const card = { backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }

function daysAgo(n: number): string {
  const d = new Date()
  d.setDate(d.getDate() - n)
  return localDate(d)
}

function Table({ title, rows }: { title: string; rows: UsageReportRow[] }) {
  const max = Math.max(1, ...rows.map((r) => r.hours))
  return (
    <div className="rounded-xl p-4 space-y-2" style={card}>
      <h3 className="text-sm font-semibold" style={{ color: 'var(--text-primary)' }}>{title}</h3>
      {rows.length === 0 ? (
        <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>Sin sesiones en el periodo</p>
      ) : rows.map((r) => (
        <div key={r.key} className="space-y-0.5">
          <div className="flex justify-between gap-2 text-xs" style={{ color: 'var(--text-primary)' }}>
            <span className="truncate">{r.label}</span>
            <span className="shrink-0" style={{ color: 'var(--text-secondary)' }}>{r.hours} h · {r.sessions} ses.</span>
          </div>
          <div className="h-1.5 rounded-full" style={{ backgroundColor: 'var(--bg-tertiary)' }}>
            <div className="h-full rounded-full" style={{ width: `${(r.hours / max) * 100}%`, backgroundColor: 'var(--accent)' }} />
          </div>
        </div>
      ))}
    </div>
  )
}

/** Informe de horas de uso real por maquina, persona y proyecto (staff) */
export default function UsageReportView() {
  const { addToast } = useToast()
  const [from, setFrom] = useState(() => daysAgo(30))
  const [to, setTo] = useState(() => daysAgo(0))
  const [rep, setRep] = useState<UsageReport | null>(null)
  const [loading, setLoading] = useState(true)

  const load = useCallback(async () => {
    setLoading(true)
    try {
      setRep(await fetchUsageReport(from, to))
    } catch (e) {
      addToast((e as Error).message, 'error')
    } finally {
      setLoading(false)
    }
  }, [from, to, addToast])

  useEffect(() => { load() }, [load])

  return (
    <div className="space-y-4">
      <div className="flex items-center gap-2 flex-wrap text-sm" style={{ color: 'var(--text-secondary)' }}>
        <span>Desde</span>
        <input type="date" value={from} max={to} onChange={(e) => e.target.value && setFrom(e.target.value)} className="px-2 py-1 rounded-lg text-sm" style={inputStyle} />
        <span>hasta</span>
        <input type="date" value={to} min={from} onChange={(e) => e.target.value && setTo(e.target.value)} className="px-2 py-1 rounded-lg text-sm" style={inputStyle} />
        {rep && <span className="ml-auto" style={{ color: 'var(--text-primary)' }}><b>{rep.total_hours} h</b> en {rep.sessions} sesiones</span>}
      </div>
      {loading && !rep ? (
        <div className="flex justify-center py-16"><Loader2 size={24} className="animate-spin" style={{ color: 'var(--accent)' }} /></div>
      ) : rep && (
        <div className="grid gap-4 grid-cols-1 lg:grid-cols-3">
          <Table title="Por maquina" rows={rep.by_machine} />
          <Table title="Por persona" rows={rep.by_user} />
          <Table title="Por proyecto" rows={rep.by_project} />
        </div>
      )}
    </div>
  )
}

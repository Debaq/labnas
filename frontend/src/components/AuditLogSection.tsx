import { useCallback, useEffect, useState } from 'react'
import { ScrollText, Search, Loader2 } from 'lucide-react'
import { fetchAudit, type AuditEvent } from '../api'

const PAGE = 100

function formatTime(ts: string): string {
  const d = new Date(ts)
  return isNaN(d.getTime()) ? ts : d.toLocaleString()
}

/** Admin: registro de auditoria persistente, con filtro por usuario y texto */
export default function AuditLogSection() {
  const [events, setEvents] = useState<AuditEvent[]>([])
  const [user, setUser] = useState('')
  const [q, setQ] = useState('')
  const [loading, setLoading] = useState(false)
  const [hasMore, setHasMore] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const load = useCallback(async (append: boolean, beforeId?: number) => {
    setLoading(true)
    setError(null)
    try {
      const page = await fetchAudit({ user: user.trim(), q: q.trim(), before_id: beforeId, limit: PAGE })
      setEvents((prev) => (append ? [...prev, ...page] : page))
      setHasMore(page.length === PAGE)
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    } finally {
      setLoading(false)
    }
  }, [user, q])

  useEffect(() => {
    load(false)
    // Solo al montar; los filtros se aplican con el boton / Enter
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  const inputStyle = { backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }

  return (
    <section>
      <div className="flex items-center gap-3 mb-4">
        <ScrollText size={22} style={{ color: 'var(--accent)' }} />
        <h2 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>
          Auditoria
        </h2>
      </div>
      <div
        className="rounded-xl p-6 space-y-4"
        style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}
      >
        <div className="flex flex-wrap gap-2">
          <input
            value={user}
            onChange={(e) => setUser(e.target.value)}
            onKeyDown={(e) => { if (e.key === 'Enter') load(false) }}
            placeholder="Usuario"
            className="w-36 px-3 py-2 rounded-lg text-sm outline-none"
            style={inputStyle}
          />
          <input
            value={q}
            onChange={(e) => setQ(e.target.value)}
            onKeyDown={(e) => { if (e.key === 'Enter') load(false) }}
            placeholder="Buscar en accion o detalle"
            className="flex-1 min-w-48 px-3 py-2 rounded-lg text-sm outline-none"
            style={inputStyle}
          />
          <button
            onClick={() => load(false)}
            disabled={loading}
            className="flex items-center gap-1 px-3 py-2 rounded-lg text-sm disabled:opacity-50"
            style={{ backgroundColor: 'var(--accent)', color: 'var(--bg-primary)' }}
          >
            {loading ? <Loader2 size={15} className="animate-spin" /> : <Search size={15} />}
            Filtrar
          </button>
        </div>

        {error && <p className="text-sm" style={{ color: 'var(--danger)' }}>{error}</p>}

        <div className="overflow-auto max-h-[480px] rounded-lg" style={{ border: '1px solid var(--border)' }}>
          <table className="w-full text-sm">
            <thead className="sticky top-0" style={{ backgroundColor: 'var(--bg-tertiary)' }}>
              <tr style={{ color: 'var(--text-secondary)' }}>
                <th className="text-left px-3 py-2 font-medium whitespace-nowrap">Fecha</th>
                <th className="text-left px-3 py-2 font-medium">Usuario</th>
                <th className="text-left px-3 py-2 font-medium">Accion</th>
                <th className="text-left px-3 py-2 font-medium">Detalle</th>
              </tr>
            </thead>
            <tbody>
              {events.map((ev) => (
                <tr key={ev.id} style={{ borderTop: '1px solid var(--border)', color: 'var(--text-primary)' }}>
                  <td className="px-3 py-1.5 whitespace-nowrap" style={{ color: 'var(--text-secondary)' }}>{formatTime(ev.timestamp)}</td>
                  <td className="px-3 py-1.5 whitespace-nowrap">{ev.username}</td>
                  <td className="px-3 py-1.5 whitespace-nowrap">{ev.action}</td>
                  <td className="px-3 py-1.5 break-all">{ev.details}</td>
                </tr>
              ))}
              {events.length === 0 && !loading && (
                <tr>
                  <td colSpan={4} className="px-3 py-6 text-center" style={{ color: 'var(--text-secondary)' }}>
                    Sin eventos
                  </td>
                </tr>
              )}
            </tbody>
          </table>
        </div>

        {hasMore && (
          <button
            onClick={() => load(true, events[events.length - 1]?.id)}
            disabled={loading}
            className="w-full py-2 rounded-lg text-sm disabled:opacity-50"
            style={{ backgroundColor: 'var(--bg-tertiary)', color: 'var(--text-secondary)' }}
          >
            Cargar mas
          </button>
        )}
      </div>
    </section>
  )
}

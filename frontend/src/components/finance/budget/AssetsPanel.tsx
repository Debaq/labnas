import { useEffect, useState } from 'react'
import { Link } from 'react-router-dom'
import { Loader2 } from 'lucide-react'
import { fetchAssets, type Assets } from '../../../api'
import { useToast } from '../../ToastContext'
import { errorMessage } from '../../../lib/errors'
import { money, type FinanceTabProps } from '../../../lib/finance'
import { cardStyle } from '../../../lib/financeBudget'

const TH = 'text-left py-2 px-2 font-medium text-xs'

/** Bienes de Inventario asociados al proyecto y horas de uso de máquinas */
export default function AssetsPanel({ projectId, projects }: FinanceTabProps) {
  const { addToast } = useToast()
  const [data, setData] = useState<Assets | null>(null)
  useEffect(() => {
    fetchAssets(projectId).then(setData).catch((e) => addToast(errorMessage(e), 'error'))
  }, [projectId, addToast])
  if (!data) return <div className="flex justify-center py-10"><Loader2 className="animate-spin" /></div>
  const title = (id: string) => projects.find((p) => p.id === id)?.title ?? id

  return (
    <div className="space-y-4">
      <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>
        Los bienes se asignan a un proyecto desde <Link to="/inventory" className="underline">Inventario</Link> (proyecto, número de inventario y responsable).
      </p>
      {data.items.length === 0 ? (
        <p className="text-sm text-center py-8" style={{ color: 'var(--text-secondary)' }}>No hay bienes asociados {projectId ? 'a este proyecto' : 'a ningún proyecto'}.</p>
      ) : (
        <div className="rounded-xl overflow-x-auto" style={cardStyle}>
          <table className="w-full text-sm" style={{ color: 'var(--text-primary)' }}>
            <thead><tr style={{ borderBottom: '1px solid var(--border)', color: 'var(--text-secondary)' }}>
              <th className={TH}>N° inventario</th><th className={TH}>Bien</th>{!projectId && <th className={TH}>Proyecto</th>}<th className={TH}>Responsable</th><th className={TH}>Ubicación</th><th className={TH}>Cant.</th><th className={TH}>Costo total</th>
            </tr></thead>
            <tbody>
              {data.items.map((a) => (
                <tr key={a.id} style={{ borderBottom: '1px solid var(--border)' }}>
                  <td className="py-2 px-2 font-mono text-xs">{a.inventory_number || '—'}</td>
                  <td className="px-2">{a.name}</td>
                  {!projectId && <td className="px-2 text-xs">{title(a.project_id)}</td>}
                  <td className="px-2 text-xs">{a.responsible || '—'}</td>
                  <td className="px-2 text-xs">{a.location || '—'}</td>
                  <td className="px-2 font-mono">{a.quantity}</td>
                  <td className="px-2 font-mono">{money(a.total_cost)}</td>
                </tr>
              ))}
            </tbody>
            <tfoot><tr><td colSpan={projectId ? 5 : 6} className="py-2 px-2 text-right text-xs font-semibold">Costo total de los bienes</td><td className="px-2 font-mono font-semibold" style={{ color: 'var(--text-primary)' }}>{money(data.total_cost)}</td></tr></tfoot>
          </table>
        </div>
      )}
      {projectId && (
        <div className="rounded-xl p-4 space-y-2" style={cardStyle}>
          <h3 className="text-sm font-semibold" style={{ color: 'var(--text-primary)' }}>Horas de uso de máquinas</h3>
          {data.usage.length === 0 ? (
            <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>
              Ninguna sesión de uso declaró este proyecto. Las máquinas se vinculan por el nombre del proyecto que se escribe al empezar el uso.
            </p>
          ) : (
            <ul className="text-sm space-y-1" style={{ color: 'var(--text-primary)' }}>
              {data.usage.map((u) => <li key={u.machine}>{u.machine}: {u.hours} h en {u.sessions} {u.sessions === 1 ? 'sesión' : 'sesiones'}</li>)}
              <li className="font-semibold">Total: {data.usage_hours.toFixed(1)} h</li>
            </ul>
          )}
        </div>
      )}
    </div>
  )
}

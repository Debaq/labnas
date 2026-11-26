import { useCallback, useEffect, useState } from 'react'
import { Loader2 } from 'lucide-react'
import { fetchAnomalies, type AnomalyReport } from '../../../api'
import { useToast } from '../../ToastContext'
import { errorMessage } from '../../../lib/errors'
import { money, type FinanceTabProps } from '../../../lib/finance'
import { FINDING_LABEL, SEVERITY_COLOR, cardStyle } from '../../../lib/financeRendicion'

/** Informe de anomalias (solo lectura): cosas que merecen una mirada humana */
export default function AnomaliasSection({ projectId }: FinanceTabProps) {
  const { addToast } = useToast()
  const [report, setReport] = useState<AnomalyReport | null>(null)
  const [loading, setLoading] = useState(true)

  const load = useCallback(async () => {
    try {
      setReport(await fetchAnomalies(projectId))
    } catch (e) {
      addToast(errorMessage(e), 'error')
    } finally {
      setLoading(false)
    }
  }, [projectId, addToast])
  // eslint-disable-next-line react-hooks/set-state-in-effect
  useEffect(() => { void load() }, [load])

  if (loading) return <div className="flex justify-center py-8"><Loader2 className="animate-spin" /></div>
  if (!report) return null
  return (
    <div className="space-y-3">
      <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>
        Revision automatica, solo lectura. Un hallazgo no es una acusacion: es algo que conviene mirar. Umbral de cotizaciones usado: {money(report.threshold)}.
      </p>
      {report.findings.length === 0 && <p className="text-sm" style={{ color: 'var(--text-secondary)' }}>Sin hallazgos.</p>}
      {report.findings.map((f, i) => (
        <div key={i} className="rounded-xl p-3 text-sm" style={{ ...cardStyle, borderLeft: `4px solid ${SEVERITY_COLOR[f.severity]}` }}>
          <div className="flex items-center gap-2 flex-wrap">
            <span className="text-[11px] px-2 py-0.5 rounded-full" style={{ color: SEVERITY_COLOR[f.severity], border: '1px solid currentColor' }}>{FINDING_LABEL[f.kind] ?? f.kind}</span>
            <span className="font-medium" style={{ color: 'var(--text-primary)' }}>{f.title}</span>
          </div>
          <p className="mt-1" style={{ color: 'var(--text-secondary)' }}>{f.detail}</p>
          {f.purchase_id && (
            <a className="text-xs mt-1 inline-block" style={{ color: 'var(--accent)' }}
              href={`/finanzas?tab=compras${f.project_id ? `&proyecto=${encodeURIComponent(f.project_id)}` : ''}&compra=${encodeURIComponent(f.purchase_id)}`}>
              Ver la compra
            </a>
          )}
        </div>
      ))}
    </div>
  )
}

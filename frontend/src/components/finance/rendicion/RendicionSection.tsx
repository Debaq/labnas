import { useCallback, useEffect, useState } from 'react'
import { Download, FileSpreadsheet, FileText, Loader2, Printer } from 'lucide-react'
import { fetchRendicion, finFileUrl, rendicionExportUrl, rendicionPrintUrl, type Rendicion } from '../../../api'
import { useEvent } from '../../../events/useEvents'
import { useToast } from '../../ToastContext'
import { errorMessage } from '../../../lib/errors'
import { money, type FinanceTabProps } from '../../../lib/finance'
import { cardStyle, inputStyle } from '../../../lib/financeRendicion'

/** Rendicion por proyecto y periodo: gastos con documento y ejecucion por item */
export default function RendicionSection({ projectId }: FinanceTabProps) {
  const { addToast } = useToast()
  const [from, setFrom] = useState('')
  const [to, setTo] = useState('')
  const [data, setData] = useState<Rendicion | null>(null)
  const [loading, setLoading] = useState(true)

  const load = useCallback(async () => {
    try {
      setData(await fetchRendicion({ projectId, from, to }))
    } catch (e) {
      setData(null)
      addToast(errorMessage(e), 'error')
    } finally {
      setLoading(false)
    }
  }, [projectId, from, to, addToast])

  // eslint-disable-next-line react-hooks/set-state-in-effect
  useEffect(() => { void load() }, [load])
  useEvent('finance', () => { void load() }, () => { void load() })

  const f = { projectId, from, to }
  const btn = 'inline-flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-sm'
  return (
    <div className="space-y-4">
      <div className="rounded-xl p-3 flex flex-wrap items-end gap-3" style={cardStyle}>
        <label className="text-xs" style={{ color: 'var(--text-secondary)' }}>Desde
          <input type="date" value={from} onChange={(e) => setFrom(e.target.value)} className="block rounded-lg px-2 py-1 text-sm" style={inputStyle} />
        </label>
        <label className="text-xs" style={{ color: 'var(--text-secondary)' }}>Hasta
          <input type="date" value={to} onChange={(e) => setTo(e.target.value)} className="block rounded-lg px-2 py-1 text-sm" style={inputStyle} />
        </label>
        <div className="flex flex-wrap gap-2 ml-auto">
          <a href={rendicionExportUrl(f, 'xlsx')} className={btn} style={{ ...inputStyle }}><FileSpreadsheet size={14} /> Excel</a>
          <a href={rendicionExportUrl(f, 'csv')} className={btn} style={{ ...inputStyle }}><FileText size={14} /> CSV</a>
          <a href={rendicionExportUrl(f, 'zip')} className={btn} style={{ ...inputStyle }}><Download size={14} /> Excel + respaldos</a>
          <a href={rendicionPrintUrl(f)} target="_blank" rel="noreferrer" className={btn} style={{ ...inputStyle }}><Printer size={14} /> Version imprimible</a>
        </div>
      </div>
      {loading ? <div className="flex justify-center py-8"><Loader2 className="animate-spin" /></div> : !data ? null : (
        <>
          <section className="rounded-xl p-3 overflow-x-auto" style={cardStyle}>
            <h2 className="text-sm font-semibold mb-2" style={{ color: 'var(--text-primary)' }}>Gastos con documento ({data.documents.length})</h2>
            <table className="w-full text-xs" style={{ color: 'var(--text-primary)' }}>
              <thead><tr style={{ color: 'var(--text-secondary)' }} className="text-left">
                <th className="py-1 pr-2">Fecha</th><th className="pr-2">Documento</th><th className="pr-2">Emisor</th>
                <th className="pr-2 text-right">Neto</th><th className="pr-2 text-right">IVA</th><th className="pr-2 text-right">Total</th>
                <th className="pr-2">Item</th><th className="pr-2">Compra</th><th>Respaldo</th>
              </tr></thead>
              <tbody>
                {data.documents.map((d) => (
                  <tr key={d.id} style={{ borderTop: '1px solid var(--border)' }}>
                    <td className="py-1 pr-2 whitespace-nowrap">{d.issued_on ?? '—'}</td>
                    <td className="pr-2">{d.kind} {d.folio}</td>
                    <td className="pr-2">{d.issuer_name}<div style={{ color: 'var(--text-secondary)' }}>{d.issuer_rut}</div></td>
                    <td className="pr-2 text-right">{money(d.net)}</td><td className="pr-2 text-right">{money(d.tax)}</td><td className="pr-2 text-right font-medium">{money(d.total)}</td>
                    <td className="pr-2">{d.budget_item || '—'}</td>
                    <td className="pr-2">{d.purchase_folio != null ? `#${d.purchase_folio} ${d.purchase_title}` : '—'}</td>
                    <td>{d.file_id ? <a href={finFileUrl(d.file_id)} target="_blank" rel="noreferrer" style={{ color: 'var(--accent)' }}>{d.file_name}</a> : '—'}</td>
                  </tr>
                ))}
                {data.documents.length === 0 && <tr><td colSpan={9} className="py-3 text-center" style={{ color: 'var(--text-secondary)' }}>Sin documentos confirmados en el periodo.</td></tr>}
              </tbody>
              <tfoot><tr className="font-semibold" style={{ borderTop: '2px solid var(--border)' }}>
                <td colSpan={3} className="py-1">Total</td><td className="text-right pr-2">{money(data.total_net)}</td><td className="text-right pr-2">{money(data.total_tax)}</td><td className="text-right pr-2">{money(data.total)}</td><td colSpan={3} />
              </tr></tfoot>
            </table>
          </section>
          <section className="rounded-xl p-3 overflow-x-auto" style={cardStyle}>
            <h2 className="text-sm font-semibold mb-2" style={{ color: 'var(--text-primary)' }}>Gastado contra lo aprobado</h2>
            <table className="w-full text-xs" style={{ color: 'var(--text-primary)' }}>
              <thead><tr style={{ color: 'var(--text-secondary)' }} className="text-left">
                <th className="py-1 pr-2">Item</th><th className="pr-2 text-right">Aprobado</th><th className="pr-2 text-right">Comprometido</th>
                <th className="pr-2 text-right">Gastado</th><th className="pr-2 text-right">Disponible</th><th className="text-right">Documentado en el periodo</th>
              </tr></thead>
              <tbody>
                {data.items.map((i) => (
                  <tr key={i.id ?? 'sin-item'} style={{ borderTop: '1px solid var(--border)' }}>
                    <td className="py-1 pr-2">{i.name}<div style={{ color: 'var(--text-secondary)' }}>{i.category}{i.project && !projectId ? ` · ${i.project}` : ''}</div></td>
                    <td className="pr-2 text-right">{money(i.approved)}</td><td className="pr-2 text-right">{money(i.committed)}</td>
                    <td className="pr-2 text-right font-medium" style={i.over ? { color: 'var(--danger)' } : undefined}>{money(i.spent)}</td>
                    <td className="pr-2 text-right">{money(i.available)}</td><td className="text-right">{money(i.documented)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </section>
        </>
      )}
    </div>
  )
}

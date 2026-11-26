import { useState } from 'react'
import { Download, Printer, Loader2 } from 'lucide-react'
import { downloadSensorCsv, fetchSensorReport } from '../api'
import type { SensorReport } from '../api/sensors'
import { errorMessage } from '../lib/errors'
import { printSensorReport } from '../lib/sensorReport'

const iso = (d: Date) => `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`

export default function SensorExportPanel({ deviceId }: { deviceId: string }) {
  const [to, setTo] = useState(() => iso(new Date()))
  const [from, setFrom] = useState(() => { const d = new Date(); d.setMonth(d.getMonth() - 1); return iso(d) })
  const [report, setReport] = useState<SensorReport | null>(null)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')

  const run = async (fn: () => Promise<void>) => {
    setBusy(true); setError('')
    try { await fn() } catch (e) { setError(errorMessage(e)) } finally { setBusy(false) }
  }
  const input = { backgroundColor: 'var(--bg-tertiary)', color: 'var(--text-primary)', border: '1px solid var(--border)' }
  const btn = { backgroundColor: 'var(--bg-tertiary)', color: 'var(--text-primary)', border: '1px solid var(--border)' }

  return (
    <div className="space-y-2 pt-2" style={{ borderTop: '1px solid var(--border)' }}>
      <span className="text-xs font-medium" style={{ color: 'var(--text-secondary)' }}>Exportar historial</span>
      <div className="flex items-center gap-2 flex-wrap">
        <input type="date" value={from} max={to} onChange={e => setFrom(e.target.value)} className="px-2 py-1 rounded text-xs" style={input} />
        <span className="text-xs" style={{ color: 'var(--text-secondary)' }}>a</span>
        <input type="date" value={to} min={from} onChange={e => setTo(e.target.value)} className="px-2 py-1 rounded text-xs" style={input} />
        <button disabled={busy} onClick={() => run(() => downloadSensorCsv(deviceId, from, to))}
          className="flex items-center gap-1 px-2 py-1 rounded text-xs" style={btn}>
          <Download size={12} /> Descargar CSV
        </button>
        <button disabled={busy} onClick={() => run(async () => setReport(await fetchSensorReport(deviceId, from, to)))}
          className="flex items-center gap-1 px-2 py-1 rounded text-xs" style={btn}>
          {busy ? <Loader2 size={12} className="animate-spin" /> : null} Ver resumen
        </button>
        {report && (
          <button onClick={() => printSensorReport(report)} className="flex items-center gap-1 px-2 py-1 rounded text-xs" style={btn}>
            <Printer size={12} /> Imprimir informe
          </button>
        )}
      </div>
      {error && <p className="text-xs" style={{ color: 'var(--danger)' }}>{error}</p>}
      {report && (
        <div className="space-y-2 text-xs" style={{ color: 'var(--text-primary)' }}>
          {report.keys.length === 0 ? (
            <p style={{ color: 'var(--text-secondary)' }}>Sin lecturas en el periodo.</p>
          ) : (
            <div className="overflow-x-auto">
              <table className="w-full text-left">
                <thead style={{ color: 'var(--text-secondary)' }}>
                  <tr><th className="pr-3">Clave</th><th className="pr-3">Min</th><th className="pr-3">Max</th><th className="pr-3">Prom</th><th>Lecturas</th></tr>
                </thead>
                <tbody>
                  {report.keys.map(k => (
                    <tr key={k.key}>
                      <td className="pr-3">{k.key}</td><td className="pr-3">{k.min.toFixed(1)} {k.unit}</td>
                      <td className="pr-3">{k.max.toFixed(1)} {k.unit}</td><td className="pr-3">{k.avg.toFixed(1)} {k.unit}</td><td>{k.count}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
          {report.alerts.length > 0 && (
            <div className="space-y-0.5">
              {report.alerts.map(a => (
                <p key={a.alert_id}>
                  {a.key} {a.condition === 'above' ? '>' : '<'} {a.threshold}: ~{a.minutes_out_of_range} min fuera de rango
                  {a.incidents > 0 && `, ${a.incidents} incidente${a.incidents === 1 ? '' : 's'}`}
                </p>
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  )
}

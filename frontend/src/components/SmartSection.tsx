import { useEffect, useState } from 'react'
import { HeartPulse, Loader2, RefreshCw } from 'lucide-react'
import { fetchSmart, setSmartEnabled, type SmartInfo, type DiskHealth } from '../api'
import { errorMessage } from '../lib/errors'

const STATUS: Record<string, { label: string; color: string }> = {
  ok: { label: 'Bien', color: 'var(--success)' },
  warning: { label: 'Advertencia', color: 'var(--warning)' },
  failing: { label: 'Falla', color: 'var(--danger)' },
  unknown: { label: 'Sin datos', color: 'var(--text-secondary)' },
}

function details(d: DiskHealth): string {
  const parts: string[] = []
  if (d.temperature != null) parts.push(`${d.temperature} °C`)
  if (d.power_on_hours != null) parts.push(`${d.power_on_hours.toLocaleString()} h de uso`)
  if (d.percentage_used != null) parts.push(`desgaste ${d.percentage_used}%`)
  const bad = [
    ['reasignados', d.reallocated], ['pendientes', d.pending], ['irrecuperables', d.uncorrectable], ['errores de medio', d.media_errors],
  ].filter(([, v]) => (v as number | null) != null && (v as number) > 0).map(([k, v]) => `${v} ${k}`)
  return [...parts, ...bad].join(' · ')
}

/** Admin: salud de discos (SMART). Desactivado por defecto; requiere setup-smart.sh */
export default function SmartSection() {
  const [info, setInfo] = useState<SmartInfo | null>(null)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  async function run(fn: () => Promise<SmartInfo>) {
    setBusy(true)
    setError(null)
    try { setInfo(await fn()) } catch (e) { setError(errorMessage(e)) } finally { setBusy(false) }
  }

  useEffect(() => { fetchSmart().then(setInfo).catch((e) => setError(errorMessage(e))) }, [])

  return (
    <section>
      <div className="flex items-center gap-3 mb-4">
        <HeartPulse size={22} style={{ color: 'var(--accent)' }} />
        <h2 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>Salud de discos (SMART)</h2>
      </div>
      <div className="rounded-xl p-6 space-y-4" style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}>
        <div className="flex items-center justify-between gap-3 flex-wrap">
          <label className="flex items-center gap-2 text-sm" style={{ color: 'var(--text-primary)' }}>
            <input type="checkbox" checked={info?.enabled ?? false} disabled={!info || busy}
              onChange={(e) => { const v = e.target.checked; run(() => setSmartEnabled(v)) }} />
            Revisar la salud de los discos (cada 6 horas; avisa a los admins si un disco empeora)
          </label>
          {info?.enabled && (
            <button onClick={() => run(fetchSmart)} disabled={busy} className="flex items-center gap-1 px-3 py-1.5 rounded-lg text-xs"
              style={{ color: 'var(--text-secondary)', border: '1px solid var(--border)' }}>
              {busy ? <Loader2 size={13} className="animate-spin" /> : <RefreshCw size={13} />} Revisar ahora
            </button>
          )}
        </div>

        {error && <p className="text-sm" style={{ color: 'var(--danger)' }}>{error}</p>}

        {info?.enabled && !info.ready && (
          <div className="text-xs space-y-1 rounded-lg p-3" style={{ backgroundColor: 'var(--bg-tertiary)', color: 'var(--text-secondary)' }}>
            <p>LabNAS no corre como root: para leer SMART necesita un permiso de solo lectura. En el servidor, desde la carpeta de LabNAS:</p>
            <code className="block" style={{ color: 'var(--text-primary)' }}>sudo bash setup-smart.sh &lt;usuario-que-corre-labnas&gt;</code>
            <p>Instala un lector que solo ejecuta <code>smartctl -j -a</code> sobre discos reales y una regla de sudo limitada a ese lector.</p>
          </div>
        )}

        {info?.enabled && info.disks.map((d) => {
          const st = STATUS[d.status ?? 'unknown']
          return (
            <div key={d.device} className="flex items-center justify-between gap-3 px-3 py-2 rounded-lg" style={{ backgroundColor: 'var(--bg-tertiary)' }}>
              <div className="min-w-0">
                <p className="text-sm" style={{ color: 'var(--text-primary)' }}>
                  <span className="font-mono">{d.device}</span> <span style={{ color: 'var(--text-secondary)' }}>{d.model} {d.size}</span>
                </p>
                <p className="text-xs" style={{ color: d.error ? 'var(--warning)' : 'var(--text-secondary)' }}>{d.error ?? details(d)}</p>
              </div>
              <span className="text-xs px-2 py-0.5 rounded-full shrink-0" style={{ color: st.color, border: `1px solid ${st.color}` }}>{st.label}</span>
            </div>
          )
        })}
      </div>
    </section>
  )
}

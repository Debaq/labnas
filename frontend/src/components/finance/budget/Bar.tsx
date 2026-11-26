import { barColor, fmtPct } from '../../../lib/financeBudget'

/** Barra de avance: verde bajo la alerta, ámbar desde el % de alerta, rojo desde el 100% */
export default function Bar({ pct, alertPct }: { pct: number | null; alertPct: number }) {
  const w = Math.max(0, Math.min(100, pct ?? 0))
  return (
    <div className="flex items-center gap-2 min-w-[7rem]">
      <div className="flex-1 h-2 rounded-full overflow-hidden" style={{ backgroundColor: 'var(--input-bg)', border: '1px solid var(--border)' }}>
        <div className="h-full" style={{ width: `${w}%`, backgroundColor: barColor(pct, alertPct) }} />
      </div>
      <span className="text-xs font-mono w-10 text-right" style={{ color: barColor(pct, alertPct) }}>{fmtPct(pct)}</span>
    </div>
  )
}

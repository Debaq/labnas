import type { BudgetCategory, BudgetLevel } from '../api'

export const inputStyle = { backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }
export const cardStyle = { backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }

export const CATEGORY_LABEL: Record<BudgetCategory, string> = {
  personal: 'Personal',
  operacion: 'Operación',
  equipos: 'Equipos',
  viajes: 'Viajes',
  otro: 'Otro',
}

export const LEVEL: Record<BudgetLevel, { label: string; color: string }> = {
  ok: { label: 'En orden', color: 'var(--success)' },
  alerta: { label: 'En alerta', color: 'var(--warning, #d97706)' },
  tope: { label: 'Al tope', color: 'var(--danger)' },
}

/** Color de la barra de avance segun el % usado y el % de alerta del proyecto */
export function barColor(pct: number | null, alertPct: number): string {
  if (pct == null) return 'var(--text-secondary)'
  if (pct >= 100) return LEVEL.tope.color
  if (pct >= alertPct) return LEVEL.alerta.color
  return LEVEL.ok.color
}

export function fmtPct(pct: number | null): string {
  return pct == null ? '—' : `${pct.toFixed(0)}%`
}

/** `AAAA-MM-DD...` -> `DD-MM-AAAA` */
export function fmtDay(s: string): string {
  const [y, m, d] = s.slice(0, 10).split('-')
  return `${d}-${m}-${y}`
}

export function todayYmd(): string {
  const d = new Date()
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`
}

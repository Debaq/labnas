import type { PortfolioEntry } from '../types'

/** Lo que la pagina de Finanzas le pasa a cada pestaña */
export interface FinanceTabProps {
  /** Proyecto elegido arriba (null = todos) */
  projectId: string | null
  /** Proyectos del Portafolio (para elegir en formularios) */
  projects: PortfolioEntry[]
  /** Gestiona Finanzas (aprueba, configura) */
  staff: boolean
  /** Puede usar Finanzas (pedir compras, subir respaldos) */
  canUse: boolean
}

/** Pesos chilenos sin decimales: $1.234.567 */
export function money(n: number | null | undefined, currency = 'CLP'): string {
  if (n == null || !Number.isFinite(n)) return '—'
  if (currency === 'CLP') return '$' + Math.round(n).toLocaleString('es-CL')
  return n.toLocaleString('es-CL', { style: 'currency', currency })
}

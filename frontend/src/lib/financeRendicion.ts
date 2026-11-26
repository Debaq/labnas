/** Estilos y ayudas pequenas de la pestaña Rendicion y auditoria */
export const cardStyle = { backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }
export const inputStyle = { backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }

export function fmtDate(iso: string | null | undefined): string {
  if (!iso) return '—'
  const d = new Date(iso)
  return Number.isNaN(d.getTime()) ? iso : d.toLocaleString('es-CL', { dateStyle: 'short', timeStyle: 'short' })
}

export const HOUR_STATUS_LABEL: Record<string, string> = {
  pendiente: 'Pendiente',
  aprobada: 'Aprobada',
  rechazada: 'Rechazada',
  anulada: 'Anulada',
}

export const FINDING_LABEL: Record<string, string> = {
  duplicado: 'Duplicado',
  proveedor_nuevo: 'Proveedor nuevo',
  fraccionamiento: 'Fraccionamiento',
  sin_factura: 'Sin factura',
  funciones: 'Separacion de funciones',
  sobre_presupuesto: 'Sobre lo aprobado',
}

export const SEVERITY_COLOR: Record<string, string> = {
  alta: 'var(--danger)',
  media: '#d97706',
  info: 'var(--text-secondary)',
}

/** Mes actual como AAAA-MM */
export function thisMonth(): string {
  const d = new Date()
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}`
}

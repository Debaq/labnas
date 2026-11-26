import type { PurchaseStatus } from '../api/financePurchases'

export const cardStyle = { backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }
export const inputStyle = { backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }

export const STATUS: Record<PurchaseStatus, { label: string; color: string }> = {
  solicitud: { label: 'Solicitud', color: 'var(--text-secondary)' },
  cotizacion: { label: 'Cotizando', color: 'var(--accent)' },
  aprobada: { label: 'Aprobada', color: '#16a34a' },
  orden: { label: 'Orden emitida', color: '#16a34a' },
  recibida: { label: 'Recibida', color: '#0891b2' },
  facturada: { label: 'Facturada', color: '#0891b2' },
  pagada: { label: 'Pagada', color: '#7c3aed' },
  rendida: { label: 'Rendida', color: '#7c3aed' },
  rechazada: { label: 'Rechazada', color: 'var(--danger)' },
  anulada: { label: 'Anulada', color: 'var(--danger)' },
}

export const STATUS_ORDER: PurchaseStatus[] = ['solicitud', 'cotizacion', 'aprobada', 'orden', 'recibida', 'facturada', 'pagada', 'rendida', 'rechazada', 'anulada']

export const STEP_LABEL: Record<string, string> = {
  solicitud: 'Solicitud',
  cotizacion: 'Cotizacion',
  conflicto: 'Conflicto de interes declarado',
  aprobada: 'Aprobacion',
  rechazada: 'Rechazo',
  orden: 'Orden de compra',
  recibida: 'Recepcion conforme',
  recepcion_no_conforme: 'Recepcion NO conforme',
  factura_subida: 'Factura subida',
  facturada: 'Factura confirmada',
  pagada: 'Pago',
  rendida: 'Rendicion',
  anulada: 'Anulacion',
}

export const READING: Record<string, string> = {
  sii_xml: 'XML del SII (exacto)',
  ia: 'Propuesto por la IA',
  manual: 'Ingresado a mano',
}

export function fmtWhen(iso: string): string {
  const d = new Date(iso)
  return Number.isNaN(d.getTime()) ? iso : d.toLocaleString('es-CL', { dateStyle: 'short', timeStyle: 'short' })
}

export function shortHash(h: string | null | undefined): string {
  return h ? `${h.slice(0, 8)}…${h.slice(-4)}` : '—'
}

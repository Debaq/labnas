import type { ReceiptKind, TripStatus } from '../api'

export const TRIP_STATUS: Record<TripStatus, { label: string; color: string }> = {
  solicitud: { label: 'Pendiente de aprobacion', color: 'var(--warning, #d97706)' },
  aprobado: { label: 'Aprobado', color: 'var(--accent)' },
  rechazado: { label: 'Rechazado', color: 'var(--danger)' },
  anulado: { label: 'Anulado', color: 'var(--text-secondary)' },
  cerrado: { label: 'Cerrado', color: 'var(--success)' },
}

export const RECEIPT_KIND: Record<ReceiptKind, string> = {
  pasaje: 'Pasaje',
  alojamiento: 'Alojamiento',
  viatico: 'Viatico',
  otro: 'Otro gasto',
  informe: 'Adjunto del informe',
}

/** Largo minimo del informe de comision para poder cerrar (el servidor lo vuelve a exigir) */
export const MIN_TRIP_REPORT = 20

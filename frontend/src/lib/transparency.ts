import type { PortalSection, RequestStatus } from '../api'

export const SECTION_INFO: Record<PortalSection, { label: string; hint: string }> = {
  proyectos: { label: 'Proyectos', hint: 'Titulo, institucion, fuente de financiamiento, fechas y estado (los que llevan presupuesto en Finanzas)' },
  montos: { label: 'Montos', hint: 'Aprobado, comprometido y gastado por proyecto y categoria' },
  bienes: { label: 'Bienes', hint: 'Nombre, numero de inventario, costo y proyecto (sin numero de serie ni responsable)' },
  uso: { label: 'Horas de uso de maquinas', hint: 'Horas por maquina y mes, sin nombres de personas' },
  informes: { label: 'Informes', hint: 'Los informes que se suben y publican abajo' },
}

export const REQUEST_STATUS: Record<RequestStatus, { label: string; color: string }> = {
  recibida: { label: 'Recibida', color: 'var(--accent)' },
  en_curso: { label: 'En curso', color: 'var(--warning, #d97706)' },
  respondida: { label: 'Respondida', color: 'var(--success)' },
  derivada: { label: 'Derivada', color: 'var(--text-secondary)' },
  rechazada: { label: 'Rechazada', color: 'var(--danger)' },
}

export const PROJECT_STATUS: Record<string, string> = {
  planned: 'Planificado', active: 'En curso', completed: 'Terminado', cancelled: 'Cancelado', submitted: 'Postulado',
}

export const CATEGORY_LABEL: Record<string, string> = {
  personal: 'Personal', operacion: 'Operacion', equipos: 'Equipos', viajes: 'Viajes', otro: 'Otro', sin_asignar: 'Sin item de presupuesto',
}

/** Color del plazo: verde con holgura, amarillo cerca, rojo vencida */
export function deadlineColor(daysLeft: number | null): string {
  if (daysLeft == null) return 'var(--text-secondary)'
  if (daysLeft < 0) return 'var(--danger)'
  if (daysLeft <= 3) return 'var(--warning, #d97706)'
  return 'var(--success)'
}

export function deadlineLabel(daysLeft: number | null): string {
  if (daysLeft == null) return ''
  if (daysLeft < 0) return `Atrasada ${-daysLeft} dia${daysLeft === -1 ? '' : 's'} habil${daysLeft === -1 ? '' : 'es'}`
  if (daysLeft === 0) return 'Vence hoy'
  return `Quedan ${daysLeft} dia${daysLeft === 1 ? '' : 's'} habil${daysLeft === 1 ? '' : 'es'}`
}

/** `AAAA-MM-DD...` -> `DD-MM-AAAA` */
export function fmtDay(s: string | null | undefined): string {
  if (!s) return ''
  const [y, m, d] = s.slice(0, 10).split('-')
  return `${d}-${m}-${y}`
}

export function fmtSize(n: number): string {
  return n > 1048576 ? `${(n / 1048576).toFixed(1)} MB` : `${Math.max(1, Math.round(n / 1024))} KB`
}

export const cardStyle = { backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }
export const inputStyle = { backgroundColor: 'var(--input-bg, var(--bg-secondary))', border: '1px solid var(--border)', color: 'var(--text-primary)' }

import type { IncidentSeverity, IncidentStatus, MyTrainingStatus, SheetKind } from '../api'

export const SEVERITY: Record<IncidentSeverity, { label: string; color: string }> = {
  casi_accidente: { label: 'Casi accidente', color: 'var(--warning, #d97706)' },
  leve: { label: 'Leve', color: '#ea580c' },
  grave: { label: 'Grave', color: 'var(--danger)' },
}

export const INCIDENT_STATUS: Record<IncidentStatus, { label: string; color: string }> = {
  abierto: { label: 'Abierto', color: 'var(--danger)' },
  en_revision: { label: 'En revision', color: 'var(--warning, #d97706)' },
  cerrado: { label: 'Cerrado', color: 'var(--success)' },
}

export const TRAINING_STATUS: Record<MyTrainingStatus, { label: string; color: string }> = {
  vigente: { label: 'Vigente', color: 'var(--success)' },
  por_vencer: { label: 'Por vencer', color: 'var(--warning, #d97706)' },
  vencida: { label: 'Vencida', color: 'var(--danger)' },
  pendiente: { label: 'Pendiente', color: 'var(--text-secondary)' },
}

export const SHEET_KINDS: Record<SheetKind, string> = {
  material: 'Material (hoja de datos)',
  procedimiento: 'Procedimiento',
  equipo: 'Equipo',
  otro: 'Otro',
}

/** `AAAA-MM-DD` -> `DD-MM-AAAA` */
export function fmtDate(ymd: string): string {
  const [y, m, d] = ymd.slice(0, 10).split('-')
  return `${d}-${m}-${y}`
}

/** `AAAA-MM-DDTHH:MM` -> `DD-MM-AAAA HH:MM` */
export function fmtDateTime(t: string): string {
  return `${fmtDate(t)} ${t.slice(11, 16)}`
}

/** Fecha y hora local de ahora para un input datetime-local */
export function nowLocalInput(): string {
  const d = new Date()
  d.setMinutes(d.getMinutes() - d.getTimezoneOffset())
  return d.toISOString().slice(0, 16)
}

/** Lee un archivo como data URL (base64) */
export function readAsDataUrl(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader()
    reader.onload = () => resolve(reader.result as string)
    reader.onerror = () => reject(new Error('No se pudo leer el archivo'))
    reader.readAsDataURL(file)
  })
}

export const inputStyle = { backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }
export const cardStyle = { backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }

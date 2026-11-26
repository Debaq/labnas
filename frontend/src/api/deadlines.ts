import { api, jsonOrThrow } from './client'

// --- Plazos con cuenta regresiva (contadores sueltos + fechas del Portafolio) ---

export type DeadlineCategory = 'proyecto' | 'informe' | 'rendicion' | 'garantia' | 'convocatoria' | 'tesis' | 'licencia' | 'otro'
export type DeadlineLevel = 'ok' | 'pronto' | 'urgente' | 'vencido' | 'cumplido'
export type DeadlineNotify = 'creador' | 'staff' | 'todos'

export interface Deadline {
  /** Unico entre fuentes: `d:<id>`, `p:<proyecto>`, `m:<proyecto>:<hito>` */
  key: string
  source: 'manual' | 'proyecto' | 'hito' | 'induccion' | 'mantencion' | 'vencimiento' | 'capacitacion' | 'arqueo'
  /** Solo los contadores sueltos (`manual`) */
  id: string | null
  title: string
  project: string | null
  portfolio_id: string | null
  /** Categoria; los hitos del Portafolio traen `hito` */
  category: DeadlineCategory | 'hito'
  /** `YYYY-MM-DD` */
  due_date: string
  days_left: number
  level: DeadlineLevel
  notes: string
  notify: DeadlineNotify
  created_by: string | null
  done_at: string | null
  done_by: string | null
}

export interface DeadlineInput {
  title: string
  category: DeadlineCategory
  due_date: string
  notes: string
  notify: DeadlineNotify
  portfolio_id: string | null
}

const JSON_HEADERS = { 'Content-Type': 'application/json' }

export async function fetchDeadlines(done = false): Promise<Deadline[]> {
  return jsonOrThrow(await api(`/api/deadlines${done ? '?done=true' : ''}`), 'Error al obtener los plazos')
}

export async function createDeadline(data: DeadlineInput): Promise<Deadline> {
  return jsonOrThrow(
    await api('/api/deadlines', { method: 'POST', headers: JSON_HEADERS, body: JSON.stringify(data) }),
    'Error al crear el plazo',
  )
}

export async function updateDeadline(id: string, data: DeadlineInput): Promise<Deadline> {
  return jsonOrThrow(
    await api(`/api/deadlines/${encodeURIComponent(id)}`, { method: 'PUT', headers: JSON_HEADERS, body: JSON.stringify(data) }),
    'Error al guardar el plazo',
  )
}

export async function setDeadlineDone(id: string, done: boolean): Promise<Deadline> {
  return jsonOrThrow(
    await api(`/api/deadlines/${encodeURIComponent(id)}/done`, { method: 'POST', headers: JSON_HEADERS, body: JSON.stringify({ done }) }),
    'Error al actualizar el plazo',
  )
}

export async function deleteDeadline(id: string): Promise<void> {
  const res = await api(`/api/deadlines/${encodeURIComponent(id)}`, { method: 'DELETE' })
  if (!res.ok) throw new Error((await res.text()) || 'Error al eliminar el plazo')
}

export const DEADLINE_CATEGORIES: { id: DeadlineCategory | 'hito'; label: string; emoji: string }[] = [
  { id: 'proyecto', label: 'Cierre de proyecto', emoji: '🏁' },
  { id: 'hito', label: 'Hito', emoji: '📍' },
  { id: 'informe', label: 'Informe', emoji: '📄' },
  { id: 'rendicion', label: 'Rendicion', emoji: '🧾' },
  { id: 'garantia', label: 'Garantia', emoji: '🛡️' },
  { id: 'convocatoria', label: 'Convocatoria', emoji: '📣' },
  { id: 'tesis', label: 'Tesis', emoji: '🎓' },
  { id: 'licencia', label: 'Licencia o certificacion', emoji: '🔑' },
  { id: 'otro', label: 'Otro', emoji: '⏳' },
]

export function categoryInfo(c: string) {
  return DEADLINE_CATEGORIES.find((x) => x.id === c) ?? DEADLINE_CATEGORIES[DEADLINE_CATEGORIES.length - 1]
}

/** Color de la tarjeta: verde > 30 dias, amarillo <= 30, rojo <= 7 o vencido */
export function levelColor(l: DeadlineLevel): string {
  switch (l) {
    case 'ok': return 'var(--success)'
    case 'pronto': return 'var(--warning, #d97706)'
    case 'urgente':
    case 'vencido': return 'var(--danger)'
    default: return 'var(--text-secondary)'
  }
}

/** "12 dias", "Mañana", "Hoy", "Vencido hace 3 dias" */
export function countdownLabel(d: Pick<Deadline, 'days_left' | 'level'>): { big: string; small: string } {
  if (d.level === 'cumplido') return { big: '✓', small: 'cumplido' }
  const n = d.days_left
  if (n < 0) return { big: String(-n), small: -n === 1 ? 'dia de atraso' : 'dias de atraso' }
  if (n === 0) return { big: 'Hoy', small: 'vence hoy' }
  if (n === 1) return { big: '1', small: 'dia · mañana' }
  return { big: String(n), small: 'dias' }
}

const MONTHS = ['ene', 'feb', 'mar', 'abr', 'may', 'jun', 'jul', 'ago', 'sep', 'oct', 'nov', 'dic']

/** `2026-12-05` -> `5 dic 2026` */
export function longDate(ymd: string): string {
  const [y, m, d] = ymd.split('-').map(Number)
  return `${d} ${MONTHS[m - 1] ?? ''} ${y}`
}

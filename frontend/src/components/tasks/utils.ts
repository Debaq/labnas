import type { TaskStatus } from '../../types'

export const STATUS_CONFIG: Record<TaskStatus, { label: string; color: string; alpha: string }> = {
  pendiente: { label: 'Pendiente', color: 'var(--warning)', alpha: '20' },
  enprogreso: { label: 'En progreso', color: 'var(--accent)', alpha: '20' },
  completada: { label: 'Completada', color: 'var(--success)', alpha: '20' },
  rechazada: { label: 'Rechazada', color: 'var(--danger)', alpha: '20' },
}

export const STATUS_ORDER: TaskStatus[] = ['pendiente', 'enprogreso', 'completada', 'rechazada']

export const MONTH_NAMES = ['Enero', 'Febrero', 'Marzo', 'Abril', 'Mayo', 'Junio', 'Julio', 'Agosto', 'Septiembre', 'Octubre', 'Noviembre', 'Diciembre']
export const DAY_NAMES = ['Lun', 'Mar', 'Mie', 'Jue', 'Vie', 'Sab', 'Dom']

export interface CalendarCell {
  day: number
  month: number
  year: number
  dateStr: string
  isCurrentMonth: boolean
}

// Cuadricula del mes (semanas completas, lunes primero)
export function buildCalendarCells(calendarMonth: Date): CalendarCell[] {
  const year = calendarMonth.getFullYear()
  const month = calendarMonth.getMonth()
  const firstDay = new Date(year, month, 1)
  const lastDay = new Date(year, month + 1, 0)
  const startPad = (firstDay.getDay() + 6) % 7 // Lunes = 0
  const daysInMonth = lastDay.getDate()
  const prevMonth = new Date(year, month - 1, 1)
  const prevMonthLastDay = new Date(year, month, 0).getDate()

  const cells: CalendarCell[] = []
  for (let i = startPad - 1; i >= 0; i--) {
    const d = prevMonthLastDay - i
    const m = prevMonth.getMonth()
    const y = prevMonth.getFullYear()
    cells.push({ day: d, month: m, year: y, dateStr: `${y}-${String(m + 1).padStart(2, '0')}-${String(d).padStart(2, '0')}`, isCurrentMonth: false })
  }
  for (let d = 1; d <= daysInMonth; d++) {
    cells.push({ day: d, month, year, dateStr: `${year}-${String(month + 1).padStart(2, '0')}-${String(d).padStart(2, '0')}`, isCurrentMonth: true })
  }
  const remaining = 7 - (cells.length % 7)
  if (remaining < 7) {
    const nm = month + 1
    const ny = nm > 11 ? year + 1 : year
    const nmNorm = nm % 12
    for (let d = 1; d <= remaining; d++) {
      cells.push({ day: d, month: nmNorm, year: ny, dateStr: `${ny}-${String(nmNorm + 1).padStart(2, '0')}-${String(d).padStart(2, '0')}`, isCurrentMonth: false })
    }
  }
  return cells
}

export function recurrenceLabel(recurrence: string): string {
  if (recurrence === 'daily') return 'Diario'
  if (recurrence === 'weekly') return 'Semanal'
  if (recurrence === 'monthly') return 'Mensual'
  if (recurrence === 'weekdays') return 'Lun-Vie'
  if (recurrence.startsWith('days:')) {
    return recurrence.replace('days:', '').split(',').map(d => DAY_NAMES[parseInt(d)] || '').join(', ')
  }
  return recurrence
}

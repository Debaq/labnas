import type { InventoryLoan } from '../types'

/** Cuando vuelve un prestamo, en texto */
export function dueText(l: InventoryLoan) {
  return l.days_left < 0 ? `atrasado ${-l.days_left} d` : l.days_left === 0 ? 'vuelve hoy' : `vuelve el ${l.due_on}`
}

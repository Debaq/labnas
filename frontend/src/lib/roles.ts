import type { ModuleLevel } from '../types'

export const LEVELS: { id: ModuleLevel; label: string; hint: string }[] = [
  { id: 'ninguno', label: 'Sin acceso', hint: 'No ve el modulo' },
  { id: 'ver', label: 'Ver', hint: 'Solo consulta' },
  { id: 'usar', label: 'Usar', hint: 'Imprimir, reservar, encolar, crear lo propio' },
  { id: 'gestionar', label: 'Gestionar', hint: 'Editar lo de otros, aprobar, controlar equipos' },
]

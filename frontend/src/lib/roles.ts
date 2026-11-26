import type { ModuleLevel } from '../types'

/** Nombre visible de cada modulo configurable por rol */
export const MODULE_LABELS: Record<string, string> = {
  files: 'Archivos',
  network: 'Red',
  printing: 'Impresion',
  printers3d: 'Impresoras 3D',
  tasks: 'Tareas / Horario',
  notes: 'Notas',
  email: 'Correo',
  inventory: 'Inventario',
  portfolio: 'Portafolio',
  sensors: 'Sensores',
  terminal: 'Terminal',
  music: 'Musica',
  cameras: 'Camaras',
  machines: 'Maquinas',
  deadlines: 'Plazos',
}

export const LEVELS: { id: ModuleLevel; label: string; hint: string }[] = [
  { id: 'ninguno', label: 'Sin acceso', hint: 'No ve el modulo' },
  { id: 'ver', label: 'Ver', hint: 'Solo consulta' },
  { id: 'usar', label: 'Usar', hint: 'Imprimir, reservar, encolar, crear lo propio' },
  { id: 'gestionar', label: 'Gestionar', hint: 'Editar lo de otros, aprobar, controlar equipos' },
]

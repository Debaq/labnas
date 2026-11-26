import { api, jsonOrThrow } from './client'

// --- Pantalla del laboratorio (backend/src/handlers/display.rs) ---

export type DisplaySection = 'maquinas' | 'impresoras' | 'agenda' | 'sensores' | 'plazos' | 'avisos'

export const DISPLAY_SECTIONS: { id: DisplaySection; label: string; module: string }[] = [
  { id: 'maquinas', label: 'Maquinas en uso', module: 'machines' },
  { id: 'impresoras', label: 'Impresoras 3D', module: 'printers3d' },
  { id: 'agenda', label: 'Reservas de hoy', module: 'machines' },
  { id: 'sensores', label: 'Sensores', module: 'sensors' },
  { id: 'plazos', label: 'Plazos cercanos', module: 'deadlines' },
  { id: 'avisos', label: 'Avisos (fallas, mantenciones, alarmas)', module: 'machines' },
]

export interface DisplayScreen {
  token: string
  name: string
  sections: DisplaySection[]
  created_by: string
  created_at: string
  last_seen_at: string | null
}

export interface DisplayData {
  name: string
  lab_name: string
  logo_url: string
  now: string
  sections: DisplaySection[]
  machines: { name: string; kind: string; state: 'libre' | 'en_uso' | 'bloqueada' | 'fuera_servicio'; user: string | null; until: string | null; next_start: string | null; next_user: string | null }[]
  printers: { name: string; online: boolean; state: string; file: string | null; progress: number | null; remaining_secs: number | null; hotend: number | null; bed: number | null }[]
  agenda: { machine: string; user: string; start: string; end: string; kind: string; pending: boolean }[]
  sensors: { name: string; online: boolean; readings: [string, number][]; alarm: string[] }[]
  deadlines: { title: string; due_date: string; days_left: number; level: string }[]
  notices: { kind: 'falla' | 'mantencion' | 'sensor'; text: string }[]
}

export async function fetchScreens(): Promise<DisplayScreen[]> {
  return jsonOrThrow(await api('/api/display/screens'), 'Error al leer las pantallas')
}

export async function createScreen(name: string, sections: DisplaySection[]): Promise<DisplayScreen> {
  return jsonOrThrow(await api('/api/display/screens', {
    method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ name, sections }),
  }), 'Error al crear la pantalla')
}

export async function updateScreen(token: string, name: string, sections: DisplaySection[]): Promise<DisplayScreen> {
  return jsonOrThrow(await api(`/api/display/screens/${token}`, {
    method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ name, sections }),
  }), 'Error al guardar la pantalla')
}

export async function deleteScreen(token: string): Promise<void> {
  const res = await api(`/api/display/screens/${token}`, { method: 'DELETE' })
  if (!res.ok) throw new Error((await res.text()) || 'Error al revocar la pantalla')
}

/** Datos publicos de una pantalla (sin sesion) */
export async function fetchDisplay(token: string): Promise<DisplayData> {
  const res = await fetch(`/api/pantalla/${token}`)
  if (!res.ok) throw new Error(res.status === 404 ? 'Pantalla no encontrada o revocada' : 'Sin conexion con LabNAS')
  return res.json()
}

export function displayUrl(token: string): string {
  return `${window.location.origin}/pantalla/${token}`
}

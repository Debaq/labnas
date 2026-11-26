import { api, jsonOrThrow } from './client'

// --- Registro de entrada (backend/src/handlers/presence.rs) ---

export interface PresenceEntry {
  id: string
  who: string
  visitor: boolean
  affiliation: string
  registered_by: string
  entered_at: string
  left_at: string | null
  auto_closed: boolean
}

export interface PresenceOverview {
  inside: PresenceEntry[]
  me: PresenceEntry | null
  today: number
  by_affiliation: [string, number][]
  by_day: [string, number][]
  my_affiliation: string
}

const json = (body: unknown): RequestInit => ({ method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) })

export async function fetchPresence(): Promise<PresenceOverview> {
  return jsonOrThrow(await api('/api/safety/presence'), 'Error al leer quien esta')
}

export async function checkIn(affiliation: string): Promise<PresenceEntry> {
  return jsonOrThrow(await api('/api/safety/presence/in', json({ affiliation })), 'Error al marcar la entrada')
}

export async function checkOut(): Promise<PresenceEntry> {
  return jsonOrThrow(await api('/api/safety/presence/out', json({})), 'Error al marcar la salida')
}

export async function visitorIn(name: string, affiliation: string): Promise<PresenceEntry> {
  return jsonOrThrow(await api('/api/safety/presence/visitor', json({ name, affiliation })), 'Error al registrar la visita')
}

export async function markOut(id: string): Promise<PresenceEntry> {
  return jsonOrThrow(await api(`/api/safety/presence/${encodeURIComponent(id)}/out`, json({})), 'Error al marcar la salida')
}

export async function fetchPresenceHistory(from: string, to: string): Promise<PresenceEntry[]> {
  return jsonOrThrow(await api(`/api/safety/presence/history?from=${from}&to=${to}`), 'Error al leer el registro')
}

export async function fetchDoorQr(): Promise<{ url: string; svg: string }> {
  return jsonOrThrow(await api(`/api/safety/presence/qr?base=${encodeURIComponent(window.location.origin)}`), 'Error al generar el QR')
}

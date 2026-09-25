import { api } from './client'
import type { NetworkHost } from '../types'

// --- Network ---

export async function scanNetwork(): Promise<NetworkHost[]> {
  const res = await api('/api/network/scan', { method: 'POST' })
  if (!res.ok) throw new Error('Error al escanear red')
  return res.json()
}

export async function fetchHosts(): Promise<NetworkHost[]> {
  const res = await api('/api/network/hosts')
  if (!res.ok) throw new Error('Error al obtener hosts')
  return res.json()
}

export async function labelDevice(mac: string, label: string, icon?: string | null): Promise<void> {
  const res = await api(`/api/network/device/${encodeURIComponent(mac)}`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ label, icon: icon || null }),
  })
  if (!res.ok) throw new Error('Error al etiquetar dispositivo')
}

export async function unlabelDevice(mac: string): Promise<void> {
  const res = await api(`/api/network/device/${encodeURIComponent(mac)}`, { method: 'DELETE' })
  if (!res.ok) throw new Error('Error al quitar etiqueta')
}

/** Enciende un equipo por Wake-on-LAN (operador/admin) */
export async function wakeHost(mac: string): Promise<void> {
  const res = await api(`/api/network/wake/${encodeURIComponent(mac)}`, { method: 'POST' })
  if (!res.ok) throw new Error((await res.text()) || 'Error al enviar Wake-on-LAN')
}

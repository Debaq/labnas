import { api } from './client'

/** Dispositivo vinculado (labnas-tray): el token solo se ve al crearlo */
export interface DeviceToken {
  id: string
  name: string
  created_at: string
  last_used: string | null
}

async function okOrError(res: Response, fallback: string) {
  if (!res.ok) throw new Error((await res.text()) || fallback)
  return res
}

export async function fetchDeviceTokens(): Promise<DeviceToken[]> {
  return (await okOrError(await api('/api/auth/device-tokens'), 'Error cargando dispositivos')).json()
}

export async function createDeviceToken(name: string): Promise<{ id: string; name: string; token: string }> {
  const res = await api('/api/auth/device-tokens', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ name }),
  })
  return (await okOrError(res, 'Error vinculando el dispositivo')).json()
}

export async function deleteDeviceToken(id: string): Promise<void> {
  await okOrError(await api(`/api/auth/device-tokens/${encodeURIComponent(id)}`, { method: 'DELETE' }), 'Error desvinculando')
}

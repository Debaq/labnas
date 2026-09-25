import { api } from './client'

// --- Sensors ---

export async function fetchSensorDevices(): Promise<import('../types').SensorDevice[]> {
  const res = await api('/api/sensors/devices')
  if (!res.ok) throw new Error('Error al obtener dispositivos')
  return res.json()
}

export async function registerSensorDevice(data: { name: string; mac: string; device_type?: string; connection?: string }): Promise<import('../types').SensorDevice> {
  const res = await api('/api/sensors/devices', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(data),
  })
  if (!res.ok) throw new Error('Error al registrar dispositivo')
  return res.json()
}

export async function deleteSensorDevice(id: string): Promise<void> {
  const res = await api(`/api/sensors/devices/${id}`, { method: 'DELETE' })
  if (!res.ok) throw new Error('Error al eliminar dispositivo')
}

export async function updateSensorDeviceStatus(id: string, status: string): Promise<void> {
  const res = await api(`/api/sensors/devices/${id}/status`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ status }),
  })
  if (!res.ok) throw new Error('Error al actualizar estado')
}

export async function updateSensorDeviceName(id: string, name: string): Promise<void> {
  const res = await api(`/api/sensors/devices/${id}/name`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ name }),
  })
  if (!res.ok) throw new Error('Error al actualizar nombre')
}

export async function fetchSensorReadings(deviceId: string, params?: { key?: string; from?: string; to?: string; limit?: number }): Promise<import('../types').SensorReading[]> {
  const q = new URLSearchParams()
  if (params?.key) q.set('key', params.key)
  if (params?.from) q.set('from', params.from)
  if (params?.to) q.set('to', params.to)
  if (params?.limit) q.set('limit', String(params.limit))
  const qs = q.toString()
  const res = await api(`/api/sensors/devices/${deviceId}/readings${qs ? '?' + qs : ''}`)
  if (!res.ok) throw new Error('Error al obtener lecturas')
  return res.json()
}

export async function fetchSensorLatest(): Promise<import('../types').SensorLatest[]> {
  const res = await api('/api/sensors/latest')
  if (!res.ok) throw new Error('Error al obtener datos de sensores')
  return res.json()
}

export async function fetchSensorAlerts(): Promise<import('../types').SensorAlert[]> {
  const res = await api('/api/sensors/alerts')
  if (!res.ok) throw new Error('Error al obtener alertas')
  return res.json()
}

export async function createSensorAlert(data: { device_id: string; key: string; condition?: string; threshold: number; cooldown_minutes?: number }): Promise<import('../types').SensorAlert> {
  const res = await api('/api/sensors/alerts', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(data),
  })
  if (!res.ok) throw new Error('Error al crear alerta')
  return res.json()
}

export async function updateSensorAlert(id: string, data: Partial<import('../types').SensorAlert>): Promise<void> {
  const res = await api(`/api/sensors/alerts/${id}`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(data),
  })
  if (!res.ok) throw new Error('Error al actualizar alerta')
}

export async function deleteSensorAlert(id: string): Promise<void> {
  const res = await api(`/api/sensors/alerts/${id}`, { method: 'DELETE' })
  if (!res.ok) throw new Error('Error al eliminar alerta')
}

export async function configureReceiver(port: string): Promise<void> {
  const res = await api('/api/sensors/receiver/config', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ port }),
  })
  if (!res.ok) throw new Error('Error al configurar receptor')
}

export async function fetchReceiverStatus(): Promise<import('../types').ReceiverStatus> {
  const res = await api('/api/sensors/receiver/status')
  if (!res.ok) throw new Error('Error al obtener estado del receptor')
  return res.json()
}

/** Genera (o reemplaza) el token del dispositivo; se muestra una sola vez */
export async function createSensorToken(id: string): Promise<string> {
  const res = await api(`/api/sensors/devices/${id}/token`, { method: 'POST' })
  if (!res.ok) throw new Error((await res.text()) || 'Error al generar token')
  return (await res.json()).token
}

export async function deleteSensorToken(id: string): Promise<void> {
  const res = await api(`/api/sensors/devices/${id}/token`, { method: 'DELETE' })
  if (!res.ok) throw new Error((await res.text()) || 'Error al quitar token')
}

export async function fetchSensorSecurity(): Promise<{ require_token: boolean }> {
  const res = await api('/api/sensors/security')
  if (!res.ok) throw new Error('Error al obtener seguridad de sensores')
  return res.json()
}

export async function setSensorSecurity(requireToken: boolean): Promise<{ require_token: boolean }> {
  const res = await api('/api/sensors/security', {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ require_token: requireToken }),
  })
  if (!res.ok) throw new Error((await res.text()) || 'Error al guardar')
  return res.json()
}

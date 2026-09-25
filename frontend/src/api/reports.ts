import { api } from './client'

// --- User Reports ---

export async function fetchReportsConfig(): Promise<{ enabled: boolean }> {
  const res = await api('/api/reports/config')
  if (!res.ok) throw new Error('Error')
  return res.json()
}

export async function setReportsConfig(enabled: boolean): Promise<void> {
  await api('/api/reports/config', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ enabled }),
  })
}

export async function fetchReports(): Promise<import('../types').UserReport[]> {
  const res = await api('/api/reports')
  if (!res.ok) throw new Error('Error')
  return res.json()
}

export async function createReport(data: { report_type: string; title: string; description: string }): Promise<import('../types').UserReport> {
  const res = await api('/api/reports', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(data),
  })
  if (!res.ok) throw new Error('Error creando reporte')
  return res.json()
}

export async function respondReport(id: string, data: { status: string; admin_response?: string }): Promise<import('../types').UserReport> {
  const res = await api(`/api/reports/${id}`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(data),
  })
  if (!res.ok) throw new Error('Error respondiendo reporte')
  return res.json()
}

export async function deleteReport(id: string): Promise<void> {
  const res = await api(`/api/reports/${id}`, { method: 'DELETE' })
  if (!res.ok) throw new Error('Error eliminando reporte')
}

export async function fetchMyReports(username: string): Promise<import('../types').UserReport[]> {
  const res = await api(`/api/reports/mine?user=${encodeURIComponent(username)}`)
  if (!res.ok) throw new Error('Error')
  return res.json()
}

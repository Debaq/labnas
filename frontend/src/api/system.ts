import { api, jsonOrThrow } from './client'
import type { DiskInfo, SystemInfo } from '../types'

// --- Sistema ---

export async function setUploadLimit(limitMb: number): Promise<void> {
  const res = await api('/api/system/upload-limit', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ limit_mb: limitMb }),
  })
  if (!res.ok) throw new Error((await res.text()) || 'Error al guardar limite')
}

// --- System ---

export async function fetchDisks(): Promise<DiskInfo[]> {
  const res = await api('/api/system/disks')
  if (!res.ok) throw new Error('Error al obtener discos')
  return res.json()
}

export async function fetchSystemInfo(): Promise<SystemInfo> {
  const res = await api('/api/system/info')
  if (!res.ok) throw new Error('Error al obtener info del sistema')
  return res.json()
}

export interface HealthInfo {
  status: string
  version: string
  uptime: string
  ip: string | null
  upload_limit_mb: number
}

export async function fetchHealth(): Promise<HealthInfo> {
  const res = await api('/api/health')
  if (!res.ok) throw new Error('Error al obtener estado')
  return res.json()
}

export async function shutdownServer(): Promise<void> {
  await fetch('/api/system/shutdown', { method: 'POST' })
}

export async function fetchAutostartStatus(): Promise<import('../types').AutostartStatus> {
  const res = await api('/api/system/autostart')
  if (!res.ok) throw new Error('Error al obtener estado de autostart')
  return res.json()
}

// --- Updates ---

export async function checkUpdate(): Promise<{ current_version: string; latest_version: string | null; update_available: boolean }> {
  const res = await api('/api/system/update/check')
  if (!res.ok) throw new Error('Error verificando actualizacion')
  return res.json()
}

export async function forceCheckUpdate(): Promise<{ current_version: string; latest_version: string | null; update_available: boolean }> {
  const res = await api('/api/system/update/force-check', { method: 'POST' })
  if (!res.ok) throw new Error('Error verificando actualizacion')
  return res.json()
}

export async function doUpdate(): Promise<string> {
  const res = await api('/api/system/update/do', { method: 'POST' })
  const text = await res.text()
  if (!res.ok) throw new Error(text || 'Error al actualizar')
  return text
}

export async function doReinstall(): Promise<string> {
  const res = await api('/api/system/reinstall', { method: 'POST' })
  const text = await res.text()
  if (!res.ok) throw new Error(text || 'Error al reinstalar')
  return text
}

export interface RollbackStatus {
  available: boolean
  version: string | null
}

export async function fetchRollbackStatus(): Promise<RollbackStatus> {
  const res = await api('/api/system/rollback')
  if (!res.ok) throw new Error('Error al consultar version anterior')
  return res.json()
}

export async function doRollback(): Promise<string> {
  const res = await api('/api/system/rollback', { method: 'POST' })
  const text = await res.text()
  if (!res.ok) throw new Error(text || 'Error al restaurar')
  return text
}

// --- Branding ---

export interface LabBranding {
  lab_name: string
  institution: string
  logo_url: string
  mission: string
  vision: string
  website: string
  contact_email: string
  location: string
  accent_color: string
}

export async function getBranding(): Promise<LabBranding> {
  const res = await api('/api/system/branding')
  if (!res.ok) throw new Error('Error al obtener branding')
  return res.json()
}

export async function setBranding(data: LabBranding): Promise<LabBranding> {
  const res = await api('/api/system/branding', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(data),
  })
  if (!res.ok) throw new Error('Error al guardar branding')
  return res.json()
}

// --- Servicios del lab ---

export interface LabService {
  name: string
  port: number
  description: string
  icon: string
}

export async function getServices(): Promise<LabService[]> {
  const res = await api('/api/system/services')
  if (!res.ok) return []
  return res.json()
}

export async function addService(data: LabService): Promise<LabService[]> {
  const res = await api('/api/system/services', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(data),
  })
  if (!res.ok) { const t = await res.text(); throw new Error(t) }
  return res.json()
}

export async function deleteService(port: number): Promise<void> {
  const res = await api(`/api/system/services/${port}`, { method: 'DELETE' })
  if (!res.ok) throw new Error('Error al eliminar servicio')
}

export async function updateService(port: number, data: LabService): Promise<LabService> {
  const res = await api(`/api/system/services/${port}`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(data),
  })
  if (!res.ok) throw new Error('Error al actualizar servicio')
  return res.json()
}

// --- mDNS ---

export async function getMdnsStatus(): Promise<{ enabled: boolean; hostname: string; url: string }> {
  const res = await api('/api/system/mdns')
  if (!res.ok) throw new Error('Error al obtener estado mDNS')
  return res.json()
}

export async function setMdns(enabled: boolean, hostname?: string): Promise<{ enabled: boolean; hostname: string; url: string }> {
  const res = await api('/api/system/mdns', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ enabled, hostname }),
  })
  if (!res.ok) throw new Error('Error al configurar mDNS')
  return res.json()
}

// --- Auditoria (admin) ---

export interface AuditEvent {
  id: number
  timestamp: string
  username: string
  action: string
  details: string
}

export async function fetchAudit(params: { user?: string; q?: string; before_id?: number; limit?: number }): Promise<AuditEvent[]> {
  const qs = new URLSearchParams()
  if (params.user) qs.set('user', params.user)
  if (params.q) qs.set('q', params.q)
  if (params.before_id) qs.set('before_id', String(params.before_id))
  if (params.limit) qs.set('limit', String(params.limit))
  const res = await api(`/api/audit?${qs}`)
  if (!res.ok) throw new Error('Error al obtener auditoria')
  return res.json()
}

// --- Respaldos (admin) ---

export interface BackupJob {
  id: string
  name: string
  source: string
  destination: string
  hour: number
  minute: number
  keep: number
  include_db: boolean
  enabled: boolean
  last_run: string | null
  last_status: 'ok' | 'warning' | 'error' | 'running' | null
  last_message: string | null
  created_at: string
  running: boolean
}

export type BackupJobInput = Pick<BackupJob, 'name' | 'source' | 'destination' | 'hour' | 'minute' | 'keep' | 'include_db' | 'enabled'>

export interface BackupsInfo {
  jobs: BackupJob[]
  rsync_available: boolean
  db_backups: string[]
  db_backups_dir: string
}


export async function fetchBackups(): Promise<BackupsInfo> {
  return jsonOrThrow(await api('/api/backups'), 'Error al obtener respaldos')
}

export async function saveBackup(job: BackupJobInput, id?: string): Promise<BackupJob> {
  const res = await api(id ? `/api/backups/${id}` : '/api/backups', {
    method: id ? 'PUT' : 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(job),
  })
  return jsonOrThrow(res, 'Error al guardar respaldo')
}

export async function deleteBackup(id: string): Promise<void> {
  const res = await api(`/api/backups/${id}`, { method: 'DELETE' })
  if (!res.ok) throw new Error((await res.text()) || 'Error al eliminar respaldo')
}

export async function runBackup(id: string): Promise<void> {
  const res = await api(`/api/backups/${id}/run`, { method: 'POST' })
  if (!res.ok) throw new Error((await res.text()) || 'Error al iniciar respaldo')
}

export async function fetchBackupSnapshots(id: string): Promise<string[]> {
  return jsonOrThrow(await api(`/api/backups/${id}/snapshots`), 'Error al obtener snapshots')
}

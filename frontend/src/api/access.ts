import { api, jsonOrThrow } from './client'

// --- Niveles de acceso de un registro o archivo (backend/src/handlers/access.rs) ---

export type Visibility = 'privado' | 'autorizado' | 'publico'
/** Tipo de registro: proyecto (Portafolio), bien (Inventario), nota, plazo, archivo */
export type RecordKind = 'proyecto' | 'bien' | 'nota' | 'plazo' | 'archivo'

export interface AccessGrant {
  id: string
  username: string
  expires_at: number
  note: string
  granted_by: string
  created_at: number
  active: boolean
}

export interface AccessInfo {
  kind: RecordKind
  id: string
  title: string
  visibility: Visibility
  public_token: string | null
  grants: AccessGrant[]
}

export interface RecordView {
  kind: RecordKind
  kind_label: string
  title: string
  fields: { label: string; value: string }[]
  text: string
  download: boolean
}

export interface SharedItem {
  kind: RecordKind
  kind_label: string
  id: string
  title: string
  expires_at: number
  granted_by: string
}

const q = (kind: string, id: string) => `kind=${encodeURIComponent(kind)}&id=${encodeURIComponent(id)}`

export async function fetchAccess(kind: RecordKind, id: string): Promise<AccessInfo> {
  return jsonOrThrow(await api(`/api/access?${q(kind, id)}`), 'Error al leer el nivel de acceso')
}

export async function setVisibility(kind: RecordKind, id: string, visibility: Visibility): Promise<AccessInfo> {
  return jsonOrThrow(await api('/api/access', {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ kind, id, visibility }),
  }), 'Error al cambiar el nivel de acceso')
}

export async function addGrant(kind: RecordKind, id: string, username: string, expiresAt: number, note: string): Promise<AccessInfo> {
  return jsonOrThrow(await api('/api/access/grants', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ kind, id, username, expires_at: expiresAt, note }),
  }), 'Error al autorizar')
}

export async function deleteGrant(grantId: string): Promise<void> {
  const res = await api(`/api/access/grants/${encodeURIComponent(grantId)}`, { method: 'DELETE' })
  if (!res.ok) throw new Error((await res.text()) || 'Error al quitar la autorizacion')
}

/** Nivel de varios registros (solo los que no son privados) */
export async function fetchLevels(kind: RecordKind, ids: string[]): Promise<Record<string, Visibility>> {
  if (ids.length === 0) return {}
  // Separados por salto de linea: una ruta de archivo puede tener comas
  return jsonOrThrow(await api(`/api/access/levels?kind=${kind}&ids=${encodeURIComponent(ids.join('\n'))}`), 'Error al leer niveles')
}

export async function fetchShared(): Promise<SharedItem[]> {
  return jsonOrThrow(await api('/api/shared'), 'Error al leer lo compartido')
}

export async function fetchSharedRecord(kind: RecordKind, id: string): Promise<RecordView> {
  return jsonOrThrow(await api(`/api/shared/view?${q(kind, id)}`), 'No encontrado o sin autorizacion vigente')
}

export function sharedDownloadUrl(kind: RecordKind, id: string): string {
  return `/api/shared/download?${q(kind, id)}`
}

export async function fetchPublicRecord(token: string): Promise<RecordView> {
  const res = await fetch(`/api/public/${encodeURIComponent(token)}`)
  if (!res.ok) throw new Error('No encontrado')
  return res.json()
}

export function publicUrl(token: string): string {
  return `${window.location.origin}/p/${token}`
}

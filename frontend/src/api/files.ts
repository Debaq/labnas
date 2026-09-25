import { api } from './client'
import type { FileEntry, QuickAccess } from '../types'

// --- Papelera ---

export interface TrashItem {
  id: string
  name: string
  original_path: string
  is_dir: boolean
  size: number
  deleted_by: string
  deleted_at: string
}

export async function fetchTrash(): Promise<TrashItem[]> {
  const res = await api('/api/trash')
  if (!res.ok) throw new Error((await res.text()) || 'Error al obtener la papelera')
  return res.json()
}

/** Devuelve la ruta donde quedo restaurado */
export async function restoreTrashItem(id: string): Promise<string> {
  const res = await api(`/api/trash/${id}/restore`, { method: 'POST' })
  if (!res.ok) throw new Error((await res.text()) || 'Error al restaurar')
  return res.json()
}

export async function deleteTrashItem(id: string): Promise<void> {
  const res = await api(`/api/trash/${id}`, { method: 'DELETE' })
  if (!res.ok) throw new Error((await res.text()) || 'Error al borrar')
}

export async function emptyTrash(): Promise<number> {
  const res = await api('/api/trash', { method: 'DELETE' })
  if (!res.ok) throw new Error((await res.text()) || 'Error al vaciar la papelera')
  return res.json()
}

// --- Files ---

/** Principales: "*", "perm:write", "role:operador|observador|admin", "user:<nombre>" */
export interface RootConfig {
  path: string
  readers: string[]
  writers: string[]
}

export interface StorageRoots {
  roots: RootConfig[]
  defaults: string[]
}

export async function fetchStorageRoots(): Promise<StorageRoots> {
  const res = await api('/api/files/roots')
  if (!res.ok) throw new Error('Error al obtener raices')
  return res.json()
}

export async function saveStorageRoots(roots: RootConfig[]): Promise<StorageRoots> {
  const res = await api('/api/files/roots', {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ roots }),
  })
  if (!res.ok) throw new Error((await res.text()) || 'Error al guardar raices')
  return res.json()
}

export async function fetchFiles(path?: string): Promise<FileEntry[]> {
  const params = path ? `?path=${encodeURIComponent(path)}` : ''
  const res = await api(`/api/files${params}`)
  if (!res.ok) throw new Error('Error al obtener archivos')
  return res.json()
}

export async function uploadFile(file: File, path: string): Promise<void> {
  const formData = new FormData()
  // "path" primero: el backend escribe el archivo en streaming y necesita el destino antes
  formData.append('path', path)
  formData.append('file', file)
  const res = await api('/api/files/upload', {
    method: 'POST',
    body: formData,
  })
  if (!res.ok) throw new Error((await res.text()) || 'Error al subir archivo')
}

export interface PreviewLinks {
  url: string
  download_url: string
}

/** Link temporal (30 min) a un solo archivo, usable en <img>, <video> o descargas */
export async function createPreviewToken(path: string): Promise<PreviewLinks> {
  const res = await api('/api/files/preview-token', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ path }),
  })
  if (!res.ok) throw new Error((await res.text()) || 'Error al preparar el archivo')
  return res.json()
}

/** Descarga via link temporal: el navegador la maneja en streaming (sin cargarla en memoria) */
export async function downloadFile(path: string): Promise<void> {
  const { download_url } = await createPreviewToken(path)
  const a = document.createElement('a')
  a.href = download_url
  a.download = path.split('/').pop() || 'archivo'
  document.body.appendChild(a)
  a.click()
  a.remove()
}

export async function deleteFile(path: string): Promise<void> {
  const res = await api(`/api/files?path=${encodeURIComponent(path)}`, {
    method: 'DELETE',
  })
  if (!res.ok) throw new Error('Error al eliminar archivo')
}

export async function createDirectory(path: string): Promise<void> {
  const res = await api('/api/files/directory', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ path }),
  })
  if (!res.ok) throw new Error('Error al crear directorio')
}

export async function fetchQuickAccess(): Promise<QuickAccess[]> {
  const res = await api('/api/files/quickaccess')
  if (!res.ok) throw new Error('Error al obtener accesos rapidos')
  return res.json()
}

// --- File Sharing ---

export async function createShare(path: string, expiresHours?: number): Promise<{ token: string; url: string; expires_hours: number }> {
  const res = await api('/api/shares', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ path, expires_hours: expiresHours || 24 }),
  })
  if (!res.ok) throw new Error('Error al compartir')
  return res.json()
}

export async function fetchShares(): Promise<import('../types/notes').ShareLink[]> {
  const res = await api('/api/shares')
  if (!res.ok) throw new Error('Error al obtener links')
  return res.json()
}

export async function deleteShare(token: string): Promise<void> {
  const res = await api(`/api/shares/${token}`, { method: 'DELETE' })
  if (!res.ok) throw new Error('Error al eliminar link')
}

// --- Download URL ---

export async function downloadFromUrl(url: string, destination: string): Promise<string> {
  const res = await api('/api/download-url', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ url, destination }),
  })
  if (!res.ok) throw new Error('Error al descargar')
  return res.text()
}

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

// Tamaño de parte para subidas grandes (por el puente lo limita el hosting)
let chunkSize: Promise<number> | null = null
function getChunkSize(): Promise<number> {
  if (!chunkSize) {
    chunkSize = api('/api/health').then(r => r.json()).then((h: { max_request?: number }) =>
      h.max_request ? Math.max(256 * 1024, Math.min(4 * 1024 * 1024, Math.floor(h.max_request * 0.8))) : 8 * 1024 * 1024,
    ).catch(() => 4 * 1024 * 1024)
  }
  return chunkSize
}

/** Subida por partes: cualquier tamaño en pedidos chicos (y reanuda si una parte falla) */
async function uploadChunked(file: File, path: string, size: number): Promise<void> {
  const init = await api('/api/files/upload/chunked', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ path, name: file.name, size: file.size }),
  })
  if (!init.ok) throw new Error((await init.text()) || 'Error al iniciar la subida')
  const { id } = await init.json() as { id: string }
  let offset = 0
  let retries = 0
  while (offset < file.size) {
    const part = file.slice(offset, offset + size)
    const res = await api(`/api/files/upload/chunked/${id}?offset=${offset}`, {
      method: 'PUT',
      headers: { 'Content-Type': 'application/octet-stream' },
      body: part,
    }).catch(() => null)
    if (res?.ok) {
      offset += part.size
      retries = 0
    } else if (res?.status === 409) {
      // El servidor indica desde donde seguir
      offset = parseInt(await res.text(), 10) || offset
    } else if (++retries > 3) {
      await api(`/api/files/upload/chunked/${id}`, { method: 'DELETE' }).catch(() => {})
      throw new Error((res && await res.text()) || 'Error de red al subir')
    }
  }
  const done = await api(`/api/files/upload/chunked/${id}/complete`, { method: 'POST' })
  if (!done.ok) throw new Error((await done.text()) || 'Error al terminar la subida')
}

export async function uploadFile(file: File, path: string): Promise<void> {
  const size = await getChunkSize()
  if (file.size > size) return uploadChunked(file, path, size)
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

// El backend devuelve la ruta relativa: se completa con el origen con que se entro
// (http/https, IP, nombre .local o Tailscale, puerto)
function absoluteShareUrl(url: string): string {
  return new URL(url, window.location.origin).href
}

export async function createShare(path: string, expiresHours?: number): Promise<{ token: string; url: string; expires_hours: number }> {
  const res = await api('/api/shares', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ path, expires_hours: expiresHours || 24 }),
  })
  if (!res.ok) throw new Error('Error al compartir')
  const data = await res.json()
  return { ...data, url: absoluteShareUrl(data.url) }
}

export async function fetchShares(): Promise<import('../types/notes').ShareLink[]> {
  const res = await api('/api/shares')
  if (!res.ok) throw new Error('Error al obtener links')
  const list: import('../types/notes').ShareLink[] = await res.json()
  return list.map((s) => ({ ...s, url: absoluteShareUrl(s.url) }))
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

// --- WebDAV (admin) ---

export async function fetchWebdavSettings(): Promise<{ enabled: boolean }> {
  const res = await api('/api/files/webdav')
  if (!res.ok) throw new Error('Error al obtener WebDAV')
  return res.json()
}

export async function saveWebdavSettings(enabled: boolean): Promise<{ enabled: boolean }> {
  const res = await api('/api/files/webdav', {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ enabled }),
  })
  if (!res.ok) throw new Error((await res.text()) || 'Error al guardar WebDAV')
  return res.json()
}

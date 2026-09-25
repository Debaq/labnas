import { api } from './client'
import type { Printer3DConfig, Printer3DStatus, AddPrinter3DRequest, DetectPrintersResult } from '../types'

// --- Printers 3D ---

export async function fetchPrinters3D(): Promise<Printer3DConfig[]> {
  const res = await api('/api/printers3d')
  if (!res.ok) throw new Error('Error al obtener impresoras 3D')
  return res.json()
}

export async function addPrinter3D(printer: AddPrinter3DRequest): Promise<Printer3DConfig> {
  const res = await api('/api/printers3d', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(printer),
  })
  if (!res.ok) throw new Error('Error al agregar impresora 3D')
  return res.json()
}

export async function deletePrinter3D(id: string): Promise<void> {
  const res = await api(`/api/printers3d/${id}`, { method: 'DELETE' })
  if (!res.ok) throw new Error('Error al eliminar impresora 3D')
}

export async function updatePrinter3D(id: string, data: Partial<Omit<import('../types').Printer3DConfig, 'id'>>): Promise<import('../types').Printer3DConfig> {
  const res = await api(`/api/printers3d/${id}`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(data),
  })
  if (!res.ok) throw new Error('Error al actualizar impresora 3D')
  return res.json()
}

export async function fetchPrinter3DStatus(id: string): Promise<Printer3DStatus> {
  const res = await api(`/api/printers3d/${id}/status`)
  if (!res.ok) throw new Error('Error al obtener estado de impresora')
  return res.json()
}

export async function uploadGcode(id: string, file: File): Promise<void> {
  const formData = new FormData()
  formData.append('file', file)
  const res = await api(`/api/printers3d/${id}/upload`, {
    method: 'POST',
    body: formData,
  })
  if (!res.ok) throw new Error('Error al subir gcode')
}

export async function detectPrinters3D(): Promise<DetectPrintersResult[]> {
  const res = await api('/api/printers3d/detect', { method: 'POST' })
  if (!res.ok) throw new Error('Error al detectar impresoras')
  return res.json()
}

export async function testHome3D(ip: string, port: number, printer_type: string, api_key?: string): Promise<string> {
  const res = await api('/api/printers3d/test-home', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ ip, port, printer_type, api_key: api_key || null }),
  })
  if (!res.ok) throw new Error('Error al enviar Home')
  return res.text()
}

export async function controlPrint3D(id: string, command: 'start' | 'pause' | 'resume' | 'cancel'): Promise<string> {
  const res = await api(`/api/printers3d/${id}/control`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ command }),
  })
  if (!res.ok) throw new Error('Error al controlar impresion')
  return res.text()
}

export async function preheat3D(id: string, hotend: number, bed: number): Promise<string> {
  const res = await api(`/api/printers3d/${id}/preheat`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ hotend, bed }),
  })
  if (!res.ok) throw new Error('Error al precalentar')
  return res.text()
}

export async function homeAxes3D(id: string, axes?: string[]): Promise<string> {
  const res = await api(`/api/printers3d/${id}/home`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ axes: axes || [] }),
  })
  if (!res.ok) throw new Error('Error al hacer home')
  return res.text()
}

export async function jog3D(id: string, x: number, y: number, z: number): Promise<string> {
  const res = await api(`/api/printers3d/${id}/jog`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ x, y, z }),
  })
  if (!res.ok) throw new Error('Error al mover ejes')
  return res.text()
}

export async function sendGcode3D(id: string, command: string): Promise<string> {
  const res = await api(`/api/printers3d/${id}/gcode`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ command }),
  })
  if (!res.ok) throw new Error('Error al enviar G-code')
  return res.text()
}

export async function fetchPrinterFiles(id: string): Promise<import('../types').PrinterFileInfo[]> {
  const res = await api(`/api/printers3d/${id}/files`)
  if (!res.ok) throw new Error('Error al obtener archivos')
  return res.json()
}

export async function printFile3D(id: string, filename: string): Promise<string> {
  const res = await api(`/api/printers3d/${id}/files/${encodeURIComponent(filename)}/print`, {
    method: 'POST',
  })
  if (!res.ok) throw new Error('Error al imprimir archivo')
  return res.text()
}

export async function deletePrinterFile(id: string, filename: string): Promise<void> {
  const res = await api(`/api/printers3d/${id}/files/${encodeURIComponent(filename)}`, {
    method: 'DELETE',
  })
  if (!res.ok) throw new Error('Error al eliminar archivo')
}

export function cameraSnapshotUrl(id: string): string {
  return `/api/printers3d/${id}/camera`
}

// Secciones de impresoras 3D
export async function fetchPrinter3DSections(): Promise<import('../types').Printer3DSection[]> {
  const res = await api('/api/printers3d/sections')
  if (!res.ok) throw new Error('Error al obtener secciones')
  return res.json()
}

export async function addPrinter3DSection(name: string): Promise<import('../types').Printer3DSection> {
  const res = await api('/api/printers3d/sections', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ name }),
  })
  if (!res.ok) throw new Error('Error al crear seccion')
  return res.json()
}

export async function updatePrinter3DSection(id: string, data: { name?: string; order?: number }): Promise<import('../types').Printer3DSection> {
  const res = await api(`/api/printers3d/sections/${id}`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(data),
  })
  if (!res.ok) throw new Error('Error al actualizar seccion')
  return res.json()
}

export async function deletePrinter3DSection(id: string): Promise<void> {
  const res = await api(`/api/printers3d/sections/${id}`, { method: 'DELETE' })
  if (!res.ok) throw new Error('Error al eliminar seccion')
}

export async function reorderPrinter3D(id: string, sectionId: string | null, order: number): Promise<import('../types').Printer3DConfig> {
  const res = await api(`/api/printers3d/${id}/reorder`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ section_id: sectionId, order }),
  })
  if (!res.ok) throw new Error('Error al reordenar impresora')
  return res.json()
}

export async function reorderPrinter3DSections(order: string[]): Promise<void> {
  const res = await api('/api/printers3d/sections/reorder', {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(order),
  })
  if (!res.ok) throw new Error('Error al reordenar secciones')
}

// --- Cola compartida ---

export interface QueueItem {
  id: string
  title: string
  file_name: string
  printer_id: string | null
  requested_by: string
  notes: string
  grams: number | null
  seconds: number | null
  status: 'pendiente' | 'imprimiendo' | 'terminado' | 'cancelado'
  position: number
  created_at: string
  updated_at: string
}

export type QueueInput = Pick<QueueItem, 'title' | 'file_name' | 'notes'> & {
  printer_id?: string | null
  grams?: number | null
  seconds?: number | null
}

async function queueJson<T>(res: Response, fallback: string): Promise<T> {
  if (!res.ok) throw new Error((await res.text()) || fallback)
  return res.json()
}

export async function fetchPrintQueue(): Promise<QueueItem[]> {
  return queueJson(await api('/api/printers3d/queue'), 'Error al obtener la cola')
}

export async function addToPrintQueue(item: QueueInput): Promise<QueueItem> {
  return queueJson(await api('/api/printers3d/queue', {
    method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(item),
  }), 'Error al pedir la impresion')
}

export async function updatePrintQueueItem(id: string, patch: Partial<QueueInput> & { status?: QueueItem['status'] }): Promise<QueueItem> {
  return queueJson(await api(`/api/printers3d/queue/${id}`, {
    method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(patch),
  }), 'Error al actualizar el pedido')
}

export async function deletePrintQueueItem(id: string): Promise<void> {
  const res = await api(`/api/printers3d/queue/${id}`, { method: 'DELETE' })
  if (!res.ok) throw new Error((await res.text()) || 'Error al borrar el pedido')
}

export async function reorderPrintQueue(ids: string[]): Promise<void> {
  const res = await api('/api/printers3d/queue/reorder', {
    method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ ids }),
  })
  if (!res.ok) throw new Error((await res.text()) || 'Error al ordenar la cola')
}

// --- Timelapse ---

export interface TimelapseSettings {
  printers: string[]
  interval_secs: number
  dir: string
  effective_dir: string
  ffmpeg: boolean
}

export async function fetchTimelapse(): Promise<TimelapseSettings> {
  return queueJson(await api('/api/printers3d/timelapse'), 'Error al obtener timelapse')
}

export async function saveTimelapse(cfg: Pick<TimelapseSettings, 'printers' | 'interval_secs' | 'dir'>): Promise<TimelapseSettings> {
  return queueJson(await api('/api/printers3d/timelapse', {
    method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(cfg),
  }), 'Error al guardar timelapse')
}

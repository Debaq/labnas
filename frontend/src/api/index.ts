import type {
  FileEntry,
  NetworkHost,
  DiskInfo,
  SystemInfo,
  QuickAccess,
  Printer3DConfig,
  Printer3DStatus,
  AddPrinter3DRequest,
  DetectPrintersResult,
  CupsPrinter,
  CupsPrintJob,
  PrintFileRequest,
} from '../types'

// --- Files ---

export async function fetchFiles(path?: string): Promise<FileEntry[]> {
  const params = path ? `?path=${encodeURIComponent(path)}` : ''
  const res = await fetch(`/api/files${params}`)
  if (!res.ok) throw new Error('Error al obtener archivos')
  return res.json()
}

export async function uploadFile(file: File, path?: string): Promise<void> {
  const formData = new FormData()
  formData.append('file', file)
  if (path) formData.append('path', path)
  const res = await fetch('/api/files/upload', {
    method: 'POST',
    body: formData,
  })
  if (!res.ok) throw new Error('Error al subir archivo')
}

export function downloadFile(path: string): void {
  const url = `/api/files/download?path=${encodeURIComponent(path)}`
  window.open(url, '_blank')
}

export async function deleteFile(path: string): Promise<void> {
  const res = await fetch(`/api/files?path=${encodeURIComponent(path)}`, {
    method: 'DELETE',
  })
  if (!res.ok) throw new Error('Error al eliminar archivo')
}

export async function createDirectory(path: string): Promise<void> {
  const res = await fetch('/api/files/directory', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ path }),
  })
  if (!res.ok) throw new Error('Error al crear directorio')
}

export async function fetchQuickAccess(): Promise<QuickAccess[]> {
  const res = await fetch('/api/files/quickaccess')
  if (!res.ok) throw new Error('Error al obtener accesos rapidos')
  return res.json()
}

// --- System ---

export async function fetchDisks(): Promise<DiskInfo[]> {
  const res = await fetch('/api/system/disks')
  if (!res.ok) throw new Error('Error al obtener discos')
  return res.json()
}

export async function fetchSystemInfo(): Promise<SystemInfo> {
  const res = await fetch('/api/system/info')
  if (!res.ok) throw new Error('Error al obtener info del sistema')
  return res.json()
}

export async function fetchHealth(): Promise<any> {
  const res = await fetch('/api/health')
  if (!res.ok) throw new Error('Error al obtener estado')
  return res.json()
}

export async function shutdownServer(): Promise<void> {
  await fetch('/api/system/shutdown', { method: 'POST' })
}

export async function fetchAutostartStatus(): Promise<import('../types').AutostartStatus> {
  const res = await fetch('/api/system/autostart')
  if (!res.ok) throw new Error('Error al obtener estado de autostart')
  return res.json()
}

export async function installAutostart(): Promise<void> {
  const res = await fetch('/api/system/autostart', { method: 'POST' })
  if (!res.ok) {
    const text = await res.text()
    throw new Error(text || 'Error instalando autostart')
  }
}

export async function removeAutostart(): Promise<void> {
  const res = await fetch('/api/system/autostart', { method: 'DELETE' })
  if (!res.ok) throw new Error('Error removiendo autostart')
}

// --- Network ---

export async function scanNetwork(): Promise<NetworkHost[]> {
  const res = await fetch('/api/network/scan', { method: 'POST' })
  if (!res.ok) throw new Error('Error al escanear red')
  return res.json()
}

export async function fetchHosts(): Promise<NetworkHost[]> {
  const res = await fetch('/api/network/hosts')
  if (!res.ok) throw new Error('Error al obtener hosts')
  return res.json()
}

// --- Printers 3D ---

export async function fetchPrinters3D(): Promise<Printer3DConfig[]> {
  const res = await fetch('/api/printers3d')
  if (!res.ok) throw new Error('Error al obtener impresoras 3D')
  return res.json()
}

export async function addPrinter3D(printer: AddPrinter3DRequest): Promise<Printer3DConfig> {
  const res = await fetch('/api/printers3d', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(printer),
  })
  if (!res.ok) throw new Error('Error al agregar impresora 3D')
  return res.json()
}

export async function deletePrinter3D(id: string): Promise<void> {
  const res = await fetch(`/api/printers3d/${id}`, { method: 'DELETE' })
  if (!res.ok) throw new Error('Error al eliminar impresora 3D')
}

export async function fetchPrinter3DStatus(id: string): Promise<Printer3DStatus> {
  const res = await fetch(`/api/printers3d/${id}/status`)
  if (!res.ok) throw new Error('Error al obtener estado de impresora')
  return res.json()
}

export async function uploadGcode(id: string, file: File): Promise<void> {
  const formData = new FormData()
  formData.append('file', file)
  const res = await fetch(`/api/printers3d/${id}/upload`, {
    method: 'POST',
    body: formData,
  })
  if (!res.ok) throw new Error('Error al subir gcode')
}

export async function detectPrinters3D(): Promise<DetectPrintersResult[]> {
  const res = await fetch('/api/printers3d/detect', { method: 'POST' })
  if (!res.ok) throw new Error('Error al detectar impresoras')
  return res.json()
}

// --- Notifications ---

export async function fetchNotificationConfig(): Promise<import('../types').NotificationConfig> {
  const res = await fetch('/api/notifications/whatsapp')
  if (!res.ok) throw new Error('Error al obtener config de notificaciones')
  return res.json()
}

export async function addWhatsAppContact(contact: { name: string; phone: string; apikey: string }): Promise<void> {
  const res = await fetch('/api/notifications/whatsapp', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(contact),
  })
  if (!res.ok) throw new Error('Error al agregar contacto')
}

export async function deleteWhatsAppContact(phone: string): Promise<void> {
  const res = await fetch(`/api/notifications/whatsapp/${encodeURIComponent(phone)}`, { method: 'DELETE' })
  if (!res.ok) throw new Error('Error al eliminar contacto')
}

export async function sendTestWhatsApp(): Promise<string> {
  const res = await fetch('/api/notifications/whatsapp/test', { method: 'POST' })
  return res.text()
}

export async function setNotificationSchedule(schedule: { daily_enabled: boolean; daily_hour: number; daily_minute: number }): Promise<void> {
  const res = await fetch('/api/notifications/schedule', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(schedule),
  })
  if (!res.ok) throw new Error('Error al configurar horario')
}

// --- CUPS Printing ---

export async function fetchCupsPrinters(): Promise<CupsPrinter[]> {
  const res = await fetch('/api/printing/printers')
  if (!res.ok) throw new Error('Error al obtener impresoras CUPS')
  return res.json()
}

export async function printFileUpload(file: File, printer: string, options?: {
  copies?: number
  orientation?: string
  double_sided?: boolean
  pages?: string
}): Promise<void> {
  const formData = new FormData()
  formData.append('file', file)
  formData.append('printer', printer)
  if (options?.copies) formData.append('copies', options.copies.toString())
  if (options?.orientation) formData.append('orientation', options.orientation)
  if (options?.double_sided) formData.append('double_sided', options.double_sided.toString())
  if (options?.pages) formData.append('pages', options.pages)
  const res = await fetch('/api/printing/print', {
    method: 'POST',
    body: formData,
  })
  if (!res.ok) throw new Error('Error al imprimir')
}

export async function printFilePath(req: PrintFileRequest): Promise<void> {
  const res = await fetch('/api/printing/print-file', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(req),
  })
  if (!res.ok) throw new Error('Error al imprimir archivo')
}

export async function fetchPrintJobs(): Promise<CupsPrintJob[]> {
  const res = await fetch('/api/printing/jobs')
  if (!res.ok) throw new Error('Error al obtener cola de impresion')
  return res.json()
}

export async function cancelPrintJob(id: string): Promise<void> {
  const res = await fetch(`/api/printing/jobs/${id}`, { method: 'DELETE' })
  if (!res.ok) throw new Error('Error al cancelar trabajo')
}

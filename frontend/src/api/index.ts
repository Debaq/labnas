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
  Task,
  Project,
  CalendarEvent,
} from '../types'

// Auth-aware fetch wrapper
function authHeaders(extra?: Record<string, string>): Record<string, string> {
  const headers: Record<string, string> = { ...extra }
  try {
    const saved = localStorage.getItem('labnas_auth')
    if (saved) {
      const { token } = JSON.parse(saved)
      if (token) headers['Authorization'] = `Bearer ${token}`
    }
  } catch {}
  return headers
}

async function api(url: string, opts?: RequestInit): Promise<Response> {
  const headers = authHeaders(
    opts?.headers ? Object.fromEntries(
      opts.headers instanceof Headers
        ? opts.headers.entries()
        : Object.entries(opts.headers as Record<string, string>)
    ) : undefined
  )

  // Don't set auth header for FormData (browser sets content-type with boundary)
  const isFormData = opts?.body instanceof FormData

  return fetch(url, {
    ...opts,
    headers: isFormData
      ? (headers['Authorization'] ? { Authorization: headers['Authorization'] } : {})
      : headers,
  })
}

// --- Files ---

export async function fetchFiles(path?: string): Promise<FileEntry[]> {
  const params = path ? `?path=${encodeURIComponent(path)}` : ''
  const res = await api(`/api/files${params}`)
  if (!res.ok) throw new Error('Error al obtener archivos')
  return res.json()
}

export async function uploadFile(file: File, path?: string): Promise<void> {
  const formData = new FormData()
  formData.append('file', file)
  if (path) formData.append('path', path)
  const res = await api('/api/files/upload', {
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

export async function fetchHealth(): Promise<any> {
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

// --- Notifications (Telegram) ---

export async function fetchNotificationConfig(): Promise<import('../types').NotificationConfig> {
  const res = await api('/api/notifications/telegram')
  if (!res.ok) throw new Error('Error al obtener config de notificaciones')
  return res.json()
}

export async function setBotToken(token: string): Promise<import('../types').NotificationConfig> {
  const res = await api('/api/notifications/telegram/token', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ token }),
  })
  if (!res.ok) {
    const text = await res.text()
    throw new Error(text || 'Error al configurar bot')
  }
  return res.json()
}

export async function deleteBotToken(): Promise<void> {
  const res = await api('/api/notifications/telegram/token', { method: 'DELETE' })
  if (!res.ok) throw new Error('Error al eliminar bot')
}

export async function setChatRole(chatId: number, role: string, permissions?: import('../types').UserPermissions): Promise<void> {
  const res = await api(`/api/notifications/telegram/chat/${chatId}/role`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ role, permissions }),
  })
  if (!res.ok) throw new Error('Error al cambiar rol')
}

export async function deleteTelegramChat(chatId: number): Promise<void> {
  const res = await api(`/api/notifications/telegram/chat/${chatId}`, { method: 'DELETE' })
  if (!res.ok) throw new Error('Error al eliminar chat')
}

export async function sendTestTelegram(): Promise<string> {
  const res = await api('/api/notifications/telegram/test', { method: 'POST' })
  return res.text()
}

export async function setNotificationSchedule(schedule: { daily_enabled: boolean; daily_hour: number; daily_minute: number }): Promise<void> {
  const res = await api('/api/notifications/schedule', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(schedule),
  })
  if (!res.ok) throw new Error('Error al configurar horario')
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
  return res.text()
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

// --- Password ---

export async function changePassword(currentPassword: string, newPassword: string): Promise<void> {
  const res = await api('/api/auth/password', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ current_password: currentPassword, new_password: newPassword }),
  })
  if (!res.ok) {
    const text = await res.text()
    throw new Error(text || 'Error al cambiar contrasena')
  }
}

// --- Rename ---

export async function renameUser(newUsername: string): Promise<{ token: string; username: string; role: import('../types').UserRole; permissions: import('../types').UserPermissions }> {
  const res = await api('/api/auth/rename', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ new_username: newUsername }),
  })
  if (!res.ok) {
    const text = await res.text()
    throw new Error(text || 'Error al renombrar usuario')
  }
  return res.json()
}

// --- Linking ---

export async function generateLinkCode(token: string): Promise<string> {
  const res = await api('/api/auth/link-code', {
    method: 'POST',
    headers: { Authorization: `Bearer ${token}` },
  })
  if (!res.ok) throw new Error('Error al generar codigo')
  return res.text()
}

export async function adminLinkChat(chatId: number, webUsername: string): Promise<void> {
  const res = await api(`/api/notifications/telegram/chat/${chatId}/link`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ web_username: webUsername }),
  })
  if (!res.ok) throw new Error('Error al vincular')
}

// --- Web Users ---

export async function setLastfmKey(key: string): Promise<string> {
  const res = await api('/api/music/lastfm-key', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ key }),
  })
  if (!res.ok) throw new Error('Error guardando API key de Last.fm')
  return res.text()
}

export async function clearQueue(): Promise<MusicState> {
  const res = await api('/api/music/queue/clear', { method: 'POST' })
  if (!res.ok) throw new Error('Error vaciando cola')
  return res.json()
}

export async function luckyPlay(artist: string, track: string): Promise<MusicState> {
  const res = await api('/api/music/lucky', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ artist, track }),
  })
  if (!res.ok) {
    const text = await res.text()
    throw new Error(text || 'Error con suerte')
  }
  return res.json()
}

export async function startRadio(artist: string, track: string): Promise<MusicState> {
  const res = await api('/api/music/radio', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ artist, track }),
  })
  if (!res.ok) {
    const text = await res.text()
    throw new Error(text || 'Error iniciando radio')
  }
  return res.json()
}

export async function fetchUsernames(): Promise<string[]> {
  const res = await api('/api/auth/usernames')
  if (!res.ok) throw new Error('Error al obtener usernames')
  return res.json()
}

export async function fetchWebUsers(): Promise<{ username: string; role: import('../types').UserRole; permissions: import('../types').UserPermissions }[]> {
  const res = await api('/api/auth/users')
  if (!res.ok) throw new Error('Error al obtener usuarios')
  return res.json()
}

export async function setWebUserRole(username: string, role: string, permissions?: import('../types').UserPermissions): Promise<void> {
  const res = await api(`/api/auth/users/${encodeURIComponent(username)}/role`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ role, permissions }),
  })
  if (!res.ok) throw new Error('Error al cambiar rol')
}

export async function deleteWebUser(username: string): Promise<void> {
  const res = await api(`/api/auth/users/${encodeURIComponent(username)}`, { method: 'DELETE' })
  if (!res.ok) throw new Error('Error al eliminar usuario')
}

// --- CUPS Printing ---

export async function fetchCupsPrinters(): Promise<CupsPrinter[]> {
  const res = await api('/api/printing/printers')
  if (!res.ok) throw new Error('Error al obtener impresoras CUPS')
  return res.json()
}

export async function enablePrinter(name: string): Promise<void> {
  const res = await api(`/api/printing/printers/${encodeURIComponent(name)}/enable`, { method: 'POST' })
  if (!res.ok) throw new Error('Error al habilitar impresora')
}

export async function disablePrinter(name: string): Promise<void> {
  const res = await api(`/api/printing/printers/${encodeURIComponent(name)}/disable`, { method: 'POST' })
  if (!res.ok) throw new Error('Error al deshabilitar impresora')
}

export async function fetchPrinterOptions(name: string): Promise<import('../types').PrinterOption[]> {
  const res = await api(`/api/printing/printers/${encodeURIComponent(name)}/options`)
  if (!res.ok) throw new Error('Error al obtener opciones de impresora')
  return res.json()
}

export async function printFileUpload(file: File, printer: string, opts?: {
  copies?: number
  pages?: string
  options?: Record<string, string>
}): Promise<void> {
  const formData = new FormData()
  formData.append('file', file)
  formData.append('printer', printer)
  if (opts?.copies) formData.append('copies', opts.copies.toString())
  if (opts?.pages) formData.append('pages', opts.pages)
  if (opts?.options) {
    for (const [key, value] of Object.entries(opts.options)) {
      formData.append(`opt_${key}`, value)
    }
  }
  const res = await api('/api/printing/print', {
    method: 'POST',
    body: formData,
  })
  if (!res.ok) {
    const text = await res.text().catch(() => res.statusText)
    throw new Error(text || 'Error al imprimir')
  }
}

export async function printFilePath(req: PrintFileRequest): Promise<void> {
  const res = await api('/api/printing/print-file', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(req),
  })
  if (!res.ok) throw new Error('Error al imprimir archivo')
}

export async function fetchPrintJobs(): Promise<CupsPrintJob[]> {
  const res = await api('/api/printing/jobs')
  if (!res.ok) throw new Error('Error al obtener cola de impresion')
  return res.json()
}

export async function cancelPrintJob(id: string): Promise<void> {
  const res = await api(`/api/printing/jobs/${id}`, { method: 'DELETE' })
  if (!res.ok) throw new Error('Error al cancelar trabajo')
}

export async function wakePrinter(name: string): Promise<void> {
  const res = await api(`/api/printing/printers/${encodeURIComponent(name)}/wake`, { method: 'POST' })
  if (!res.ok) throw new Error('Error al despertar impresora')
}

export async function fetchPrinterStats(name: string): Promise<import('../types').PrinterStatsResponse> {
  const res = await api(`/api/printing/printers/${encodeURIComponent(name)}/stats`)
  if (!res.ok) throw new Error('Error al obtener estadisticas')
  return res.json()
}

export async function setPrinterCosts(name: string, costs: import('../types').PrinterCosts): Promise<void> {
  const res = await api(`/api/printing/printers/${encodeURIComponent(name)}/costs`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(costs),
  })
  if (!res.ok) throw new Error('Error al guardar costos')
}

export async function resetPrinterStats(name: string): Promise<void> {
  const res = await api(`/api/printing/printers/${encodeURIComponent(name)}/stats/reset`, { method: 'POST' })
  if (!res.ok) throw new Error('Error al resetear estadisticas')
}

export async function fetchAllUserCosts(): Promise<import('../types').AllUserCostsResponse> {
  const res = await api('/api/printing/user-costs')
  if (!res.ok) throw new Error('Error al obtener costos por usuario')
  return res.json()
}

export async function fetchMyCosts(): Promise<import('../types').AllUserCostsResponse> {
  const res = await api('/api/printing/my-costs')
  if (!res.ok) throw new Error('Error al obtener mis costos')
  return res.json()
}

// --- Tasks & Projects ---

export async function fetchProjects(): Promise<Project[]> {
  const res = await api('/api/projects')
  if (!res.ok) throw new Error('Error al obtener proyectos')
  return res.json()
}

export async function createProject(data: { name: string; description?: string }): Promise<Project> {
  const res = await api('/api/projects', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(data),
  })
  if (!res.ok) throw new Error('Error al crear proyecto')
  return res.json()
}

export async function updateProject(id: string, data: {
  name?: string; description?: string; members?: string[];
  member_tags?: Record<string, string[]>
}): Promise<Project> {
  const res = await api(`/api/projects/${id}`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(data),
  })
  if (!res.ok) throw new Error('Error al actualizar proyecto')
  return res.json()
}

export async function deleteProject(id: string): Promise<void> {
  const res = await api(`/api/projects/${id}`, { method: 'DELETE' })
  if (!res.ok) throw new Error('Error al eliminar proyecto')
}

export async function fetchTasks(params?: { project?: string; status?: string }): Promise<Task[]> {
  const query = new URLSearchParams()
  if (params?.project) query.set('project', params.project)
  if (params?.status) query.set('status', params.status)
  const qs = query.toString()
  const res = await api(`/api/tasks${qs ? '?' + qs : ''}`)
  if (!res.ok) throw new Error('Error al obtener tareas')
  return res.json()
}

export async function createTask(data: {
  title: string
  project_id?: string | null
  assigned_to?: string[]
  requires_confirmation?: boolean
  insistent?: boolean
  reminder_minutes?: number
  due_date?: string | null
  due_time?: string | null
}): Promise<Task> {
  const res = await api('/api/tasks', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(data),
  })
  if (!res.ok) throw new Error('Error al crear tarea')
  return res.json()
}

export async function updateTask(id: string, data: Record<string, unknown>): Promise<Task> {
  const res = await api(`/api/tasks/${id}`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(data),
  })
  if (!res.ok) throw new Error('Error al actualizar tarea')
  return res.json()
}

export async function confirmTask(id: string, user: string): Promise<Task> {
  const res = await api(`/api/tasks/${id}/confirm`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ user }),
  })
  if (!res.ok) throw new Error('Error al confirmar tarea')
  return res.json()
}

export async function rejectTask(id: string, user: string): Promise<Task> {
  const res = await api(`/api/tasks/${id}/reject`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ user }),
  })
  if (!res.ok) throw new Error('Error al rechazar tarea')
  return res.json()
}

export async function doneTask(id: string): Promise<Task> {
  const res = await api(`/api/tasks/${id}/done`, { method: 'POST' })
  if (!res.ok) throw new Error('Error al completar tarea')
  return res.json()
}

export async function deleteTask(id: string): Promise<void> {
  const res = await api(`/api/tasks/${id}`, { method: 'DELETE' })
  if (!res.ok) throw new Error('Error al eliminar tarea')
}

export async function scheduleTask(id: string, data: { date?: string; time?: string }): Promise<CalendarEvent> {
  const res = await api(`/api/tasks/${id}/schedule`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(data),
  })
  if (!res.ok) throw new Error('Error al agendar tarea')
  return res.json()
}

// --- Calendar Events ---

export async function fetchEvents(): Promise<CalendarEvent[]> {
  const res = await api('/api/events')
  if (!res.ok) throw new Error('Error al obtener eventos')
  return res.json()
}

export async function updateEvent(id: string, data: Record<string, unknown>): Promise<CalendarEvent> {
  const res = await api(`/api/events/${id}`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(data),
  })
  if (!res.ok) throw new Error('Error al actualizar evento')
  return res.json()
}

export async function createEvent(data: {
  title: string; date: string; time: string; end_time?: string;
  description?: string; location?: string; invitees?: string[]; remind_before_min?: number;
  notify_telegram?: boolean; recurrence?: string; recurrence_end?: string | null; category?: string
}): Promise<CalendarEvent> {
  const res = await api('/api/events', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(data),
  })
  if (!res.ok) throw new Error('Error al crear evento')
  return res.json()
}

export async function deleteEvent(id: string): Promise<void> {
  const res = await api(`/api/events/${id}`, { method: 'DELETE' })
  if (!res.ok) throw new Error('Error al eliminar evento')
}

export async function acceptEvent(id: string, user: string): Promise<CalendarEvent> {
  const res = await api(`/api/events/${id}/accept`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ user }),
  })
  if (!res.ok) throw new Error('Error al aceptar evento')
  return res.json()
}

export async function declineEvent(id: string, user: string): Promise<CalendarEvent> {
  const res = await api(`/api/events/${id}/decline`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ user }),
  })
  if (!res.ok) throw new Error('Error al rechazar evento')
  return res.json()
}

// --- Event Categories ---

export async function fetchCategories(): Promise<import('../types').EventCategory[]> {
  const res = await api('/api/events/categories')
  if (!res.ok) throw new Error('Error al obtener categorias')
  return res.json()
}

export async function createCategory(name: string, color: string): Promise<import('../types').EventCategory> {
  const res = await api('/api/events/categories', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ name, color }),
  })
  if (!res.ok) throw new Error('Error al crear categoria')
  return res.json()
}

export async function updateCategory(id: string, name: string, color: string): Promise<import('../types').EventCategory> {
  const res = await api(`/api/events/categories/${id}`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ name, color }),
  })
  if (!res.ok) throw new Error('Error al actualizar categoria')
  return res.json()
}

export async function deleteCategory(id: string): Promise<void> {
  const res = await api(`/api/events/categories/${id}`, { method: 'DELETE' })
  if (!res.ok) throw new Error('Error al eliminar categoria')
}

// --- Inventory ---

export async function fetchInventoryCategories(): Promise<import('../types').InventoryCategory[]> {
  const res = await api('/api/inventory/categories')
  if (!res.ok) throw new Error('Error al obtener categorias')
  return res.json()
}

export async function createInventoryCategory(data: { name: string; icon?: string; description?: string }): Promise<import('../types').InventoryCategory> {
  const res = await api('/api/inventory/categories', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(data) })
  if (!res.ok) throw new Error('Error al crear categoria')
  return res.json()
}

export async function updateInventoryCategory(id: string, data: { name: string; icon?: string; description?: string }): Promise<import('../types').InventoryCategory> {
  const res = await api(`/api/inventory/categories/${id}`, { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(data) })
  if (!res.ok) throw new Error('Error al actualizar categoria')
  return res.json()
}

export async function deleteInventoryCategory(id: string): Promise<void> {
  const res = await api(`/api/inventory/categories/${id}`, { method: 'DELETE' })
  if (!res.ok) throw new Error('Error al eliminar categoria')
}

export async function fetchInventoryItems(): Promise<import('../types').InventoryItem[]> {
  const res = await api('/api/inventory/items')
  if (!res.ok) throw new Error('Error al obtener items')
  return res.json()
}

export async function createInventoryItem(data: Partial<import('../types').InventoryItem>): Promise<import('../types').InventoryItem> {
  const res = await api('/api/inventory/items', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(data) })
  if (!res.ok) throw new Error('Error al crear item')
  return res.json()
}

export async function updateInventoryItem(id: string, data: Partial<import('../types').InventoryItem>): Promise<import('../types').InventoryItem> {
  const res = await api(`/api/inventory/items/${id}`, { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(data) })
  if (!res.ok) throw new Error('Error al actualizar item')
  return res.json()
}

export async function deleteInventoryItem(id: string): Promise<void> {
  const res = await api(`/api/inventory/items/${id}`, { method: 'DELETE' })
  if (!res.ok) throw new Error('Error al eliminar item')
}

export async function fetchPrintHistory(): Promise<import('../types').PrintHistoryEntry[]> {
  const res = await api('/api/inventory/print-history')
  if (!res.ok) throw new Error('Error al obtener historial')
  return res.json()
}

export async function addPrintHistory(data: Partial<import('../types').PrintHistoryEntry>): Promise<import('../types').PrintHistoryEntry> {
  const res = await api('/api/inventory/print-history', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(data) })
  if (!res.ok) throw new Error('Error al registrar impresion')
  return res.json()
}

export async function deletePrintHistory(id: string): Promise<void> {
  const res = await api(`/api/inventory/print-history/${id}`, { method: 'DELETE' })
  if (!res.ok) throw new Error('Error al eliminar entrada')
}

// --- Portfolio ---

export async function fetchPortfolio(): Promise<import('../types').PortfolioEntry[]> {
  const res = await api('/api/portfolio')
  if (!res.ok) throw new Error('Error al obtener portafolio')
  return res.json()
}

export async function createPortfolioEntry(data: Partial<import('../types').PortfolioEntry>): Promise<import('../types').PortfolioEntry> {
  const res = await api('/api/portfolio', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(data),
  })
  if (!res.ok) throw new Error('Error al crear entrada')
  return res.json()
}

export async function updatePortfolioEntry(id: string, data: Partial<import('../types').PortfolioEntry>): Promise<import('../types').PortfolioEntry> {
  const res = await api(`/api/portfolio/${id}`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(data),
  })
  if (!res.ok) throw new Error('Error al actualizar entrada')
  return res.json()
}

export async function deletePortfolioEntry(id: string): Promise<void> {
  const res = await api(`/api/portfolio/${id}`, { method: 'DELETE' })
  if (!res.ok) throw new Error('Error al eliminar entrada')
}

export async function togglePortfolioRequirement(entryId: string, reqId: string): Promise<import('../types').PortfolioEntry> {
  const res = await api(`/api/portfolio/${entryId}/requirements/${reqId}/toggle`, { method: 'POST' })
  if (!res.ok) throw new Error('Error al cambiar requisito')
  return res.json()
}

export async function togglePortfolioMilestone(entryId: string, milId: string): Promise<import('../types').PortfolioEntry> {
  const res = await api(`/api/portfolio/${entryId}/milestones/${milId}/toggle`, { method: 'POST' })
  if (!res.ok) throw new Error('Error al cambiar hito')
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

// --- Notes ---

export async function fetchNotes(): Promise<import('../types/notes').Note[]> {
  const res = await api('/api/notes')
  if (!res.ok) throw new Error('Error al obtener notas')
  return res.json()
}

export async function createNote(title: string, content?: string, shared_with?: string[], is_public?: boolean): Promise<import('../types/notes').Note> {
  const res = await api('/api/notes', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ title, content: content || '', shared_with: shared_with || [], is_public: is_public || false }),
  })
  if (!res.ok) throw new Error('Error al crear nota')
  return res.json()
}

export async function updateNote(id: string, data: { title?: string; content?: string; shared_with?: string[]; is_public?: boolean }): Promise<import('../types/notes').Note> {
  const res = await api(`/api/notes/${id}`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(data),
  })
  if (!res.ok) throw new Error('Error al actualizar nota')
  return res.json()
}

export async function deleteNote(id: string): Promise<void> {
  const res = await api(`/api/notes/${id}`, { method: 'DELETE' })
  if (!res.ok) throw new Error('Error al eliminar nota')
}

// --- Email ---

export interface EmailMessage {
  uid: number
  from: string
  subject: string
  date: string
  body_preview: string
  ai_classification: string | null
  ai_summary: string | null
  ai_action: string | null
  filter_label: string | null
  filter_action: string | null
  processed: boolean
  task_created: boolean
  fetched_at: string
}

export interface EmailFilter {
  pattern: string
  action: 'prioritario' | 'normal' | 'silencioso' | 'ignorar'
  label: string
  auto_tag: string | null
}

export async function configureEmailAccount(data: { host: string; port: number; protocol: 'imap' | 'pop3'; email: string; password: string }): Promise<string> {
  const res = await api('/api/email/account', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(data),
  })
  if (!res.ok) { const t = await res.text(); throw new Error(t) }
  return res.text()
}

export async function deleteEmailAccount(): Promise<void> {
  const res = await api('/api/email/account', { method: 'DELETE' })
  if (!res.ok) throw new Error('Error al eliminar cuenta')
}

export async function fetchInbox(): Promise<EmailMessage[]> {
  const res = await api('/api/email/inbox')
  if (!res.ok) throw new Error('Error al obtener bandeja')
  return res.json()
}

export async function checkEmailNow(): Promise<string> {
  const res = await api('/api/email/check', { method: 'POST' })
  if (!res.ok) { const t = await res.text(); throw new Error(t) }
  return res.text()
}

export async function classifyEmail(uid: number): Promise<EmailMessage> {
  const res = await api(`/api/email/classify/${uid}`, { method: 'POST' })
  if (!res.ok) throw new Error('Error al clasificar')
  return res.json()
}

export async function emailToTask(uid: number): Promise<string> {
  const res = await api(`/api/email/to-task/${uid}`, { method: 'POST' })
  if (!res.ok) { const t = await res.text(); throw new Error(t) }
  return res.text()
}

export async function setGroqKey(key: string): Promise<string> {
  const res = await api('/api/email/groq-key', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ key }),
  })
  if (!res.ok) throw new Error('Error al configurar key')
  return res.text()
}

export async function fetchEmailFilters(): Promise<EmailFilter[]> {
  const res = await api('/api/email/filters')
  if (!res.ok) throw new Error('Error al obtener filtros')
  return res.json()
}

export async function addEmailFilter(filter: { pattern: string; action: string; label: string; auto_tag?: string }): Promise<void> {
  const res = await api('/api/email/filters', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(filter),
  })
  if (!res.ok) { const t = await res.text(); throw new Error(t) }
}

export async function deleteEmailFilter(pattern: string): Promise<void> {
  const res = await api(`/api/email/filters/${encodeURIComponent(pattern)}`, { method: 'DELETE' })
  if (!res.ok) throw new Error('Error al eliminar filtro')
}

// --- Music ---

export interface MusicTrack {
  id: string
  title: string
  artist: string
  thumbnail: string
  duration: number
  added_by: string | null
}

export interface MusicState {
  current: MusicTrack | null
  queue: MusicTrack[]
  started_by: string | null
  history: { id: string; title: string; artist: string; thumbnail: string; played_by: string }[]
  paused: boolean
  volume: number
  repeat: 'off' | 'all' | 'one'
  shuffle: boolean
  video: boolean
  video_screen: number | null
  elapsed: number
}

export interface PlaylistTrack {
  id: string; title: string; artist: string; thumbnail: string; duration: number; added_by: string
}
export interface Playlist {
  id: string; name: string; description: string; created_by: string
  tracks: PlaylistTrack[]; created_at: string; updated_at: string
}

export async function fetchPlaylists(): Promise<Playlist[]> {
  const res = await api('/api/music/playlists'); return res.ok ? res.json() : []
}
export async function createPlaylist(name: string, description?: string): Promise<Playlist> {
  const res = await api('/api/music/playlists', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ name, description: description || '' }) })
  if (!res.ok) throw new Error('Error al crear playlist'); return res.json()
}
export async function updatePlaylist(id: string, data: { name?: string; description?: string }): Promise<Playlist> {
  const res = await api(`/api/music/playlists/${id}`, { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(data) })
  if (!res.ok) throw new Error('Error al actualizar playlist'); return res.json()
}
export async function deletePlaylist(id: string): Promise<void> {
  await api(`/api/music/playlists/${id}`, { method: 'DELETE' })
}
export async function addTrackToPlaylist(playlistId: string, track: { id: string; title: string; artist: string; thumbnail: string; duration: number }): Promise<Playlist> {
  const res = await api(`/api/music/playlists/${playlistId}/tracks`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(track) })
  if (!res.ok) throw new Error('Error al agregar track'); return res.json()
}
export async function removeTrackFromPlaylist(playlistId: string, index: number): Promise<Playlist> {
  const res = await api(`/api/music/playlists/${playlistId}/tracks/${index}`, { method: 'DELETE' })
  if (!res.ok) throw new Error('Error al eliminar track'); return res.json()
}
export async function moveTrackInPlaylist(playlistId: string, from: number, to: number): Promise<Playlist> {
  const res = await api(`/api/music/playlists/${playlistId}/tracks/move`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ from, to }) })
  if (!res.ok) throw new Error('Error al mover track'); return res.json()
}
export async function loadPlaylist(playlistId: string): Promise<void> {
  const res = await api(`/api/music/playlists/${playlistId}/load`, { method: 'POST' })
  if (!res.ok) throw new Error('Error al cargar playlist')
}
export async function saveQueueAsPlaylist(name: string, description?: string): Promise<Playlist> {
  const res = await api('/api/music/playlists/save-queue', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ name, description: description || '' }) })
  if (!res.ok) throw new Error('Error al guardar playlist'); return res.json()
}

export async function searchMusic(q: string): Promise<MusicTrack[]> {
  const res = await api(`/api/music/search?q=${encodeURIComponent(q)}`)
  if (!res.ok) throw new Error('Error buscando musica')
  return res.json()
}

export async function playMusic(id: string): Promise<MusicState> {
  const res = await api('/api/music/play', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ id }),
  })
  if (!res.ok) throw new Error('Error reproduciendo')
  return res.json()
}

export async function nextMusic(): Promise<MusicState> {
  const res = await api('/api/music/next', { method: 'POST' })
  if (!res.ok) throw new Error('Error pasando cancion')
  return res.json()
}

export async function getCurrentMusic(): Promise<MusicState> {
  const res = await api('/api/music/current')
  if (!res.ok) throw new Error('Error obteniendo estado')
  return res.json()
}

export async function stopMusic(): Promise<MusicState> {
  const res = await api('/api/music/stop', { method: 'POST' })
  if (!res.ok) throw new Error('Error deteniendo')
  return res.json()
}

export async function pauseMusic(): Promise<MusicState> {
  const res = await api('/api/music/pause', { method: 'POST' })
  if (!res.ok) throw new Error('Error pausando')
  return res.json()
}

export async function previousMusic(): Promise<MusicState> {
  const res = await api('/api/music/previous', { method: 'POST' })
  if (!res.ok) { const t = await res.text(); throw new Error(t) }
  return res.json()
}

export async function setMusicVolume(volume: number): Promise<MusicState> {
  const res = await api('/api/music/volume', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ volume }),
  })
  if (!res.ok) throw new Error('Error ajustando volumen')
  return res.json()
}

export async function playFromQueue(index: number): Promise<MusicState> {
  const res = await api(`/api/music/queue/play/${index}`, { method: 'POST' })
  if (!res.ok) throw new Error('Error reproduciendo de cola')
  return res.json()
}

export async function moveInQueue(from: number, to: number): Promise<MusicState> {
  const res = await api('/api/music/queue/move', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ from, to }),
  })
  if (!res.ok) throw new Error('Error moviendo en cola')
  return res.json()
}

export async function toggleShuffle(): Promise<MusicState> {
  const res = await api('/api/music/shuffle', { method: 'POST' })
  if (!res.ok) throw new Error('Error')
  return res.json()
}

export async function toggleRepeat(): Promise<MusicState> {
  const res = await api('/api/music/repeat', { method: 'POST' })
  if (!res.ok) throw new Error('Error')
  return res.json()
}

export interface ScreenInfo {
  index: number
  connector: string
  name: string
  connected: boolean
}

export async function getScreens(): Promise<ScreenInfo[]> {
  const res = await api('/api/music/screens')
  if (!res.ok) return []
  return res.json()
}

export async function setMusicVideo(video: boolean, screen?: number): Promise<MusicState> {
  const res = await api('/api/music/video', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ video, screen }),
  })
  if (!res.ok) throw new Error('Error')
  return res.json()
}

export async function removeFromQueue(index: number): Promise<MusicState> {
  const res = await api('/api/music/queue', {
    method: 'DELETE',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ index }),
  })
  if (!res.ok) throw new Error('Error eliminando de cola')
  return res.json()
}

export async function recommendMusic(): Promise<MusicState> {
  const res = await api('/api/music/recommend', { method: 'POST' })
  if (!res.ok) { const t = await res.text(); throw new Error(t) }
  return res.json()
}

export async function getMpvArgs(): Promise<string[]> {
  const res = await api('/api/music/mpv-args')
  if (!res.ok) throw new Error('Error obteniendo args de mpv')
  return res.json()
}

export async function setMpvArgs(args: string[]): Promise<string[]> {
  const res = await api('/api/music/mpv-args', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(args),
  })
  if (!res.ok) throw new Error('Error guardando args de mpv')
  return res.json()
}

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

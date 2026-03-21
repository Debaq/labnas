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
      ? { Authorization: headers['Authorization'] || '' }
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

export async function labelDevice(mac: string, label: string): Promise<void> {
  const res = await api(`/api/network/device/${encodeURIComponent(mac)}`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ label }),
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
  if (!res.ok) throw new Error('Error al imprimir')
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

// --- Calendar Events ---

export async function fetchEvents(): Promise<CalendarEvent[]> {
  const res = await api('/api/events')
  if (!res.ok) throw new Error('Error al obtener eventos')
  return res.json()
}

export async function createEvent(data: {
  title: string; date: string; time: string;
  description?: string; invitees?: string[]; remind_before_min?: number
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

export async function createNote(title: string, content?: string): Promise<import('../types/notes').Note> {
  const res = await api('/api/notes', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ title, content: content || '' }),
  })
  if (!res.ok) throw new Error('Error al crear nota')
  return res.json()
}

export async function updateNote(id: string, data: { title?: string; content?: string }): Promise<import('../types/notes').Note> {
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

import { api, jsonOrThrow } from './client'

// --- Maquinas (catalogo con QR, reservas y fallas) ---

export type MachineKind = 'laser' | 'cnc' | 'impresora3d' | 'electronica' | 'otra'
export type MachineStatus = 'disponible' | 'fuera_servicio' | 'retirada'
export type BookingStatus = 'pendiente' | 'confirmada' | 'cancelada' | 'rechazada'

export interface MachineBooking {
  id: string
  machine_id: string
  machine_name: string
  username: string
  kind: 'reserva' | 'bloqueo'
  /** Hora local `YYYY-MM-DDTHH:MM` */
  start: string
  end: string
  purpose: string
  status: BookingStatus
  decided_by: string | null
  decided_at: string | null
  created_at: string
}

export interface MachineUsageSession {
  id: string
  machine_id: string
  machine_name: string
  username: string
  booking_id: string | null
  project: string
  /** Hora local `YYYY-MM-DDTHH:MM` */
  started_at: string
  ended_at: string | null
  note: string
  hours: number | null
}

export interface MachineFault {
  id: string
  machine_id: string
  reported_by: string
  description: string
  photo_id: string | null
  created_at: string
  resolved_by: string | null
  resolved_at: string | null
  resolution_note: string | null
}

export interface Machine {
  id: string
  name: string
  kind: MachineKind
  location: string
  description: string
  instructions: string
  responsible: string | null
  status: MachineStatus
  requires_approval: boolean
  /** Solo reserva quien tiene la induccion vigente (el staff no la necesita) */
  requires_certification: boolean
  max_hours: number | null
  inventory_item_id: string | null
  printer3d_id: string | null
  photo_id: string | null
  created_by: string
  created_at: string
  current: MachineBooking | null
  next: MachineBooking | null
  open_faults: MachineFault[]
  /** Sesion de uso abierta: alguien la esta usando */
  in_use: MachineUsageSession | null
}

export interface MachineDetail extends Machine {
  faults: MachineFault[]
}

export interface MachineInput {
  name: string
  kind: MachineKind
  location: string
  description: string
  instructions: string
  responsible: string | null
  status?: MachineStatus
  requires_approval: boolean
  requires_certification: boolean
  max_hours: number | null
  inventory_item_id?: string | null
  printer3d_id?: string | null
}

const JSON_HEADERS = { 'Content-Type': 'application/json' }
const post = (body: unknown): RequestInit => ({ method: 'POST', headers: JSON_HEADERS, body: JSON.stringify(body) })

export async function fetchMachines(): Promise<Machine[]> {
  return jsonOrThrow(await api('/api/machines'), 'Error al obtener las maquinas')
}

export async function fetchMachine(id: string): Promise<MachineDetail> {
  return jsonOrThrow(await api(`/api/machines/${encodeURIComponent(id)}`), 'Maquina no encontrada')
}

export async function createMachine(data: MachineInput): Promise<Machine> {
  return jsonOrThrow(await api('/api/machines', post(data)), 'Error al crear la maquina')
}

export async function updateMachine(id: string, data: MachineInput): Promise<Machine> {
  return jsonOrThrow(
    await api(`/api/machines/${encodeURIComponent(id)}`, { method: 'PUT', headers: JSON_HEADERS, body: JSON.stringify(data) }),
    'Error al guardar la maquina',
  )
}

export async function deleteMachine(id: string): Promise<void> {
  const res = await api(`/api/machines/${encodeURIComponent(id)}`, { method: 'DELETE' })
  if (!res.ok) throw new Error((await res.text()) || 'Error al eliminar la maquina')
}

/** `data`: imagen en base64 (con o sin prefijo data:) */
export async function setMachinePhoto(id: string, data: string): Promise<Machine> {
  return jsonOrThrow(await api(`/api/machines/${encodeURIComponent(id)}/photo`, post({ data })), 'Error al subir la foto')
}

export function machinePhotoUrl(photoId: string): string {
  return `/api/machines/photos/${encodeURIComponent(photoId)}`
}

/** QR que lleva a la ficha, armado con la direccion con que se abrio LabNAS */
export async function fetchMachineQr(id: string): Promise<{ url: string; svg: string }> {
  const base = window.location.origin
  return jsonOrThrow(
    await api(`/api/machines/${encodeURIComponent(id)}/qr?base=${encodeURIComponent(base)}`),
    'Error al generar el QR',
  )
}

export interface BookingFilter {
  from?: string
  to?: string
  machine?: string
  mine?: boolean
  status?: BookingStatus | 'todas'
}

export async function fetchBookings(f: BookingFilter = {}): Promise<MachineBooking[]> {
  const q = new URLSearchParams()
  if (f.from) q.set('from', f.from)
  if (f.to) q.set('to', f.to)
  if (f.machine) q.set('machine', f.machine)
  if (f.mine) q.set('mine', 'true')
  if (f.status) q.set('status', f.status)
  return jsonOrThrow(await api(`/api/machines/bookings?${q}`), 'Error al obtener las reservas')
}

export async function createBooking(
  machineId: string,
  data: { start: string; end: string; purpose: string; kind?: 'reserva' | 'bloqueo' },
): Promise<MachineBooking> {
  return jsonOrThrow(await api(`/api/machines/${encodeURIComponent(machineId)}/bookings`, post(data)), 'Error al reservar')
}

export async function bookingAction(id: string, action: 'cancel' | 'approve' | 'reject'): Promise<MachineBooking> {
  return jsonOrThrow(await api(`/api/machines/bookings/${encodeURIComponent(id)}/${action}`, post({})), 'Error al actualizar la reserva')
}

export async function reportFault(machineId: string, description: string, photo?: string): Promise<MachineFault> {
  return jsonOrThrow(
    await api(`/api/machines/${encodeURIComponent(machineId)}/faults`, post({ description, photo })),
    'Error al reportar la falla',
  )
}

// --- Induccion por maquina ---

export interface MachineCertification {
  machine_id: string
  machine_name: string
  username: string
  status: 'solicitada' | 'vigente' | 'vencida'
  requested_at: string | null
  granted_by: string | null
  granted_at: string | null
  /** `YYYY-MM-DD`; null = no vence */
  expires_on: string | null
  notes: string
}

const certsUrl = (machineId: string) => `/api/machines/${encodeURIComponent(machineId)}/certifications`

export async function fetchCertifications(machineId: string): Promise<MachineCertification[]> {
  return jsonOrThrow(await api(certsUrl(machineId)), 'Error al obtener las inducciones')
}

export async function fetchAllCertifications(q: { mine?: boolean; status?: string } = {}): Promise<MachineCertification[]> {
  const p = new URLSearchParams()
  if (q.mine) p.set('mine', 'true')
  if (q.status) p.set('status', q.status)
  return jsonOrThrow(await api(`/api/machines/certifications?${p}`), 'Error al obtener las inducciones')
}

export async function requestCertification(machineId: string, notes = ''): Promise<MachineCertification> {
  return jsonOrThrow(await api(`${certsUrl(machineId)}/request`, post({ notes })), 'Error al pedir la induccion')
}

export async function grantCertification(machineId: string, data: { username: string; expires_on?: string | null; notes?: string }): Promise<MachineCertification> {
  return jsonOrThrow(await api(certsUrl(machineId), post(data)), 'Error al dar la induccion')
}

export async function revokeCertification(machineId: string, username: string): Promise<void> {
  const res = await api(`${certsUrl(machineId)}/${encodeURIComponent(username)}`, { method: 'DELETE' })
  if (!res.ok) throw new Error((await res.text()) || 'Error al quitar la induccion')
}

export async function resolveFault(faultId: string, note: string): Promise<MachineFault> {
  return jsonOrThrow(await api(`/api/machines/faults/${encodeURIComponent(faultId)}/resolve`, post({ note })), 'Error al cerrar la falla')
}

// --- Mantenimiento y calibracion (idea 5) ---

export type MaintenanceLevel = 'ok' | 'pronto' | 'vencida'

export interface MaintenanceTask {
  id: string
  machine_id: string
  machine_name: string
  title: string
  kind: 'mantencion' | 'calibracion'
  every_days: number | null
  every_hours: number | null
  instructions: string
  /** Mientras este vencida, la maquina no admite reservas (salvo el staff) */
  blocks: boolean
  last_done_at: string | null
  last_done_hours: number
  created_at: string
  due_date: string | null
  days_left: number | null
  hours_left: number | null
  level: MaintenanceLevel
}

export interface MaintenanceLog {
  id: string
  machine_id: string
  task_id: string | null
  kind: 'mantencion' | 'calibracion' | 'reparacion' | 'nota'
  title: string
  note: string
  usage_hours: number
  photo_id: string | null
  done_by: string
  done_at: string
}

export interface MaintenanceOverview {
  usage_hours: number
  booked_hours: number
  adjust_hours: number
  tasks: MaintenanceTask[]
  log: MaintenanceLog[]
}

export interface MaintenanceTaskInput {
  title: string
  kind: 'mantencion' | 'calibracion'
  every_days: number | null
  every_hours: number | null
  instructions: string
  blocks: boolean
}

const mUrl = (id: string) => `/api/machines/${encodeURIComponent(id)}`

export async function fetchMaintenance(machineId: string): Promise<MaintenanceOverview> {
  return jsonOrThrow(await api(`${mUrl(machineId)}/maintenance`), 'Error al leer la mantencion')
}

export async function fetchAllMaintenance(): Promise<MaintenanceTask[]> {
  return jsonOrThrow(await api('/api/machines/maintenance'), 'Error al leer la mantencion')
}

export async function createMaintenanceTask(machineId: string, data: MaintenanceTaskInput): Promise<MaintenanceTask> {
  return jsonOrThrow(await api(`${mUrl(machineId)}/maintenance/tasks`, post(data)), 'Error al crear la tarea')
}

export async function updateMaintenanceTask(taskId: string, data: MaintenanceTaskInput): Promise<MaintenanceTask> {
  return jsonOrThrow(await api(`/api/machines/maintenance/tasks/${encodeURIComponent(taskId)}`, { ...post(data), method: 'PUT' }), 'Error al guardar la tarea')
}

export async function deleteMaintenanceTask(taskId: string): Promise<void> {
  const res = await api(`/api/machines/maintenance/tasks/${encodeURIComponent(taskId)}`, { method: 'DELETE' })
  if (!res.ok) throw new Error((await res.text()) || 'Error al quitar la tarea')
}

export async function addMaintenanceLog(
  machineId: string,
  data: { task_id?: string; kind?: string; title?: string; note?: string; photo?: string },
): Promise<MaintenanceOverview> {
  return jsonOrThrow(await api(`${mUrl(machineId)}/maintenance/log`, post(data)), 'Error al registrar')
}

export async function setMachineUsage(machineId: string, hours: number): Promise<MaintenanceOverview> {
  return jsonOrThrow(await api(`${mUrl(machineId)}/usage`, { ...post({ hours }), method: 'PUT' }), 'Error al fijar las horas')
}

export const MAINTENANCE_LEVEL: Record<MaintenanceLevel, { label: string; color: string }> = {
  ok: { label: 'Al dia', color: 'var(--success)' },
  pronto: { label: 'Pronto', color: 'var(--warning, #d97706)' },
  vencida: { label: 'Vencida', color: 'var(--danger)' },
}

/** Cuanto falta para una tarea, en texto */
export function maintenanceDue(t: MaintenanceTask): string {
  const parts: string[] = []
  if (t.days_left !== null) {
    parts.push(t.days_left < 0 ? `vencio hace ${-t.days_left} d` : t.days_left === 0 ? 'toca hoy' : `en ${t.days_left} d`)
  }
  if (t.hours_left !== null) {
    parts.push(t.hours_left <= 0 ? `pasada por ${Math.abs(t.hours_left)} h de uso` : `en ${t.hours_left} h de uso`)
  }
  return parts.join(' · ')
}

/** Cada cuanto se hace, en texto */
export function maintenanceEvery(t: { every_days: number | null; every_hours: number | null }): string {
  const parts: string[] = []
  if (t.every_days) parts.push(`cada ${t.every_days} dias`)
  if (t.every_hours) parts.push(`cada ${t.every_hours} h de uso`)
  return parts.join(' o ')
}

// --- Utilidades de fecha (hora local, mismo formato que el servidor) ---

const pad = (n: number) => String(n).padStart(2, '0')

/** Date -> `YYYY-MM-DD` local */
export function localDate(d: Date): string {
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`
}

/** Date -> `YYYY-MM-DDTHH:MM` local */
export function localDateTime(d: Date): string {
  return `${localDate(d)}T${pad(d.getHours())}:${pad(d.getMinutes())}`
}

/** `YYYY-MM-DDTHH:MM` -> `HH:MM` */
export function hhmm(s: string): string {
  return s.slice(11, 16)
}

/** `YYYY-MM-DDTHH:MM` -> `lun 23-11 10:00` */
export function shortDateTime(s: string): string {
  const d = new Date(s)
  if (Number.isNaN(d.getTime())) return s
  return `${d.toLocaleDateString('es-CL', { weekday: 'short' })} ${pad(d.getDate())}-${pad(d.getMonth() + 1)} ${hhmm(s)}`
}

/** Comprime una imagen a JPEG (lado mayor `max` px) y la devuelve como data URL */
export function compressImage(file: File, max = 1280, quality = 0.82): Promise<string> {
  return new Promise((resolve, reject) => {
    const url = URL.createObjectURL(file)
    const img = new Image()
    img.onload = () => {
      URL.revokeObjectURL(url)
      const scale = Math.min(1, max / Math.max(img.width, img.height))
      const canvas = document.createElement('canvas')
      canvas.width = Math.round(img.width * scale)
      canvas.height = Math.round(img.height * scale)
      const ctx = canvas.getContext('2d')
      if (!ctx) return reject(new Error('No se pudo procesar la imagen'))
      ctx.drawImage(img, 0, 0, canvas.width, canvas.height)
      resolve(canvas.toDataURL('image/jpeg', quality))
    }
    img.onerror = () => {
      URL.revokeObjectURL(url)
      reject(new Error('Imagen invalida'))
    }
    img.src = url
  })
}

export const MACHINE_KINDS: { id: MachineKind; label: string; emoji: string }[] = [
  { id: 'laser', label: 'Corte laser', emoji: '🔥' },
  { id: 'cnc', label: 'CNC / fresadora', emoji: '⚙️' },
  { id: 'impresora3d', label: 'Impresora 3D', emoji: '🧊' },
  { id: 'electronica', label: 'Electronica', emoji: '🔌' },
  { id: 'otra', label: 'Otra', emoji: '🛠️' },
]

export function kindInfo(k: string) {
  return MACHINE_KINDS.find((x) => x.id === k) ?? MACHINE_KINDS[MACHINE_KINDS.length - 1]
}

// --- Registro de uso real (sesiones empezar / terminar) ---

export interface MachineUsageList {
  open: MachineUsageSession | null
  recent: MachineUsageSession[]
}

export interface UsageReportRow {
  key: string
  label: string
  hours: number
  sessions: number
}

export interface UsageReport {
  from: string
  to: string
  total_hours: number
  sessions: number
  by_machine: UsageReportRow[]
  by_user: UsageReportRow[]
  by_project: UsageReportRow[]
}

export async function fetchMachineUsage(machineId: string): Promise<MachineUsageList> {
  return jsonOrThrow(await api(`${mUrl(machineId)}/usage`), 'Error al leer el uso')
}

export async function startMachineUsage(machineId: string, project: string): Promise<MachineUsageSession> {
  return jsonOrThrow(await api(`${mUrl(machineId)}/usage/start`, post({ project })), 'No se pudo empezar')
}

export async function stopMachineUsage(sessionId: string, note: string, project: string): Promise<MachineUsageSession> {
  return jsonOrThrow(await api(`/api/machines/usage/${encodeURIComponent(sessionId)}/stop`, post({ note, project })), 'No se pudo terminar')
}

export async function fetchUsageReport(from: string, to: string): Promise<UsageReport> {
  const q = new URLSearchParams({ from, to })
  return jsonOrThrow(await api(`/api/machines/usage/report?${q}`), 'Error al leer el informe')
}

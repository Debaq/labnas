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
  max_hours: number | null
  inventory_item_id: string | null
  printer3d_id: string | null
  photo_id: string | null
  created_by: string
  created_at: string
  current: MachineBooking | null
  next: MachineBooking | null
  open_faults: MachineFault[]
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

export async function resolveFault(faultId: string, note: string): Promise<MachineFault> {
  return jsonOrThrow(await api(`/api/machines/faults/${encodeURIComponent(faultId)}/resolve`, post({ note })), 'Error al cerrar la falla')
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

import { api, jsonOrThrow } from './client'

export interface WorkshopReg {
  id: number
  name: string
  email: string
  username: string | null
  affiliation: string
  registered_at: string
  status: 'inscrito' | 'espera' | 'cancelado'
  attended: string[]
  certificate_code: string | null
  certificate_at: string | null
}

export interface WorkshopInfo {
  capacity: number
  registration_open: boolean
  sessions: string[]
  min_attendance: number
  can_manage: boolean
  mine: WorkshopReg | null
  enrolled: number
  waitlist: number
  public_token?: string | null
  registrations?: WorkshopReg[]
}

export interface WorkshopSettingsInput {
  capacity: number
  registration_open: boolean
  sessions: string[]
  min_attendance: number
  rotate_token?: boolean
}

export interface LabInfo { lab_name: string; institution: string; logo_url: string }

export interface Certificate {
  code: string | null
  issued_at: string | null
  valid: boolean
  name: string
  workshop: string
  hours: number | null
  modality: string
  start_date: string
  end_date: string | null
  sessions: string[]
  attended: string[]
  min_attendance: number
  lab: LabInfo
}

export interface PublicWorkshop {
  title: string
  description: string
  hours: number | null
  modality: string
  start_date: string
  end_date: string | null
  sessions: string[]
  capacity: number
  enrolled: number
  full: boolean
  lab: LabInfo
}

export interface EnrollResult { status: 'inscrito' | 'espera'; waitlist_position: number }

const base = (id: string) => `/api/portfolio/${encodeURIComponent(id)}/workshop`
const JSON_H = { 'Content-Type': 'application/json' }

export async function fetchWorkshop(id: string): Promise<WorkshopInfo> {
  return jsonOrThrow(await api(base(id)), 'Error al obtener el taller')
}

export async function saveWorkshop(id: string, data: WorkshopSettingsInput): Promise<WorkshopInfo> {
  return jsonOrThrow(await api(base(id), { method: 'PUT', headers: JSON_H, body: JSON.stringify(data) }), 'Error al guardar')
}

export async function enrollWorkshop(id: string): Promise<EnrollResult> {
  return jsonOrThrow(await api(`${base(id)}/enroll`, { method: 'POST', headers: JSON_H, body: '{}' }), 'Error al inscribirse')
}

export async function cancelWorkshopEnrollment(id: string): Promise<void> {
  const res = await api(`${base(id)}/enroll`, { method: 'DELETE' })
  if (!res.ok) throw new Error((await res.text()) || 'Error al cancelar')
}

export async function myWorkshopCertificate(id: string): Promise<Certificate> {
  return jsonOrThrow(await api(`${base(id)}/my-certificate`, { method: 'POST' }), 'Error al emitir el certificado')
}

export async function setWorkshopAttendance(id: string, rid: number, attended: string[]): Promise<void> {
  const res = await api(`${base(id)}/registrations/${rid}/attendance`, { method: 'PUT', headers: JSON_H, body: JSON.stringify({ attended }) })
  if (!res.ok) throw new Error((await res.text()) || 'Error al guardar la asistencia')
}

export async function cancelWorkshopRegistration(id: string, rid: number): Promise<void> {
  const res = await api(`${base(id)}/registrations/${rid}/cancel`, { method: 'POST' })
  if (!res.ok) throw new Error((await res.text()) || 'Error al cancelar')
}

export async function issueWorkshopCertificate(id: string, rid: number): Promise<Certificate> {
  return jsonOrThrow(await api(`${base(id)}/registrations/${rid}/certificate`, { method: 'POST' }), 'Error al emitir el certificado')
}

export function workshopCsvUrl(id: string): string {
  return `${base(id)}/export.csv`
}

// --- Publicas (sin sesion) ---

export async function fetchPublicWorkshop(token: string): Promise<PublicWorkshop> {
  const res = await fetch(`/api/portfolio/taller/${encodeURIComponent(token)}`)
  if (!res.ok) throw new Error('No encontrado')
  return res.json()
}

export async function enrollPublicWorkshop(token: string, data: { name: string; email: string; affiliation: string }): Promise<EnrollResult> {
  const res = await fetch(`/api/portfolio/taller/${encodeURIComponent(token)}`, {
    method: 'POST', headers: JSON_H, body: JSON.stringify(data),
  })
  return jsonOrThrow(res, 'Error al inscribirse')
}

export async function fetchPublicCertificate(code: string): Promise<Certificate> {
  const res = await fetch(`/api/portfolio/certificado/${encodeURIComponent(code)}`)
  if (!res.ok) throw new Error('No encontrado')
  return res.json()
}

export function workshopPublicUrl(token: string): string {
  return `${window.location.origin}/taller/${token}`
}

export function certificateUrl(code: string): string {
  return `${window.location.origin}/certificado/${code}`
}

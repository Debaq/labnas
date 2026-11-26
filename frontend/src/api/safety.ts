import { api, jsonOrThrow } from './client'

// --- Seguridad laboral: incidentes, capacitaciones y fichas de seguridad ---

export type IncidentSeverity = 'casi_accidente' | 'leve' | 'grave'
export type IncidentStatus = 'abierto' | 'en_revision' | 'cerrado'
export type SheetKind = 'material' | 'procedimiento' | 'equipo' | 'otro'
export type MyTrainingStatus = 'vigente' | 'por_vencer' | 'vencida' | 'pendiente'

export interface SafetyIncident {
  id: string
  reported_by: string
  reported_at: string
  /** Hora local `YYYY-MM-DDTHH:MM` */
  occurred_at: string
  place: string
  machine_id: string | null
  machine_name: string | null
  severity: IncidentSeverity
  description: string
  has_photo: boolean
  actions: string
  status: IncidentStatus
  closed_by: string | null
  closed_at: string | null
  closing_note: string
}

export interface IncidentInput {
  severity: IncidentSeverity
  description: string
  place?: string
  occurred_at?: string | null
  machine_id?: string | null
  /** Imagen en base64 (con o sin prefijo data:) */
  photo?: string | null
  actions?: string
}

export interface SafetyTraining {
  id: string
  name: string
  description: string
  valid_months: number | null
  mandatory: boolean
  created_by: string
  created_at: string
  holders: number
}

export interface TrainingInput {
  name: string
  description: string
  valid_months: number | null
  mandatory: boolean
}

export interface TrainingRecord {
  training_id: string
  training_name: string
  username: string
  granted_by: string
  granted_on: string
  expires_on: string | null
  notes: string
  status: 'vigente' | 'vencida'
}

export interface MyTraining {
  training: SafetyTraining
  status: MyTrainingStatus
  granted_on: string | null
  expires_on: string | null
}

export interface SafetySheet {
  id: string
  title: string
  kind: SheetKind
  content: string
  url: string
  machine_id: string | null
  machine_name: string | null
  inventory_item_id: string | null
  inventory_item_name: string | null
  file_name: string | null
  file_mime: string | null
  created_by: string
  created_at: string
  updated_at: string
}

export interface SheetInput {
  title: string
  kind: SheetKind
  content: string
  url: string
  machine_id: string | null
  inventory_item_id: string | null
  file?: { name: string; data: string } | null
  remove_file?: boolean
}

export interface SafetySummary {
  open_incidents: number
  my_missing_mandatory: number
}

const JSON_HEADERS = { 'Content-Type': 'application/json' }
const send = (method: string, body: unknown): RequestInit => ({ method, headers: JSON_HEADERS, body: JSON.stringify(body) })
const enc = encodeURIComponent

async function okOrThrow(res: Response, fallback: string): Promise<void> {
  if (!res.ok) throw new Error((await res.text()) || fallback)
}

export async function fetchSafetySummary(): Promise<SafetySummary> {
  return jsonOrThrow(await api('/api/safety/summary'), 'Error al obtener el resumen')
}

export async function fetchIncidents(): Promise<SafetyIncident[]> {
  return jsonOrThrow(await api('/api/safety/incidents'), 'Error al obtener los incidentes')
}

export async function reportIncident(data: IncidentInput): Promise<SafetyIncident> {
  return jsonOrThrow(await api('/api/safety/incidents', send('POST', data)), 'Error al reportar el incidente')
}

export async function updateIncident(id: string, data: { status: IncidentStatus; actions: string; closing_note: string }): Promise<SafetyIncident> {
  return jsonOrThrow(await api(`/api/safety/incidents/${enc(id)}`, send('PUT', data)), 'Error al guardar el incidente')
}

export function incidentPhotoUrl(id: string): string {
  return `/api/safety/incidents/${enc(id)}/photo`
}

export async function fetchTrainings(): Promise<SafetyTraining[]> {
  return jsonOrThrow(await api('/api/safety/trainings'), 'Error al obtener las capacitaciones')
}

export async function fetchMyTrainings(): Promise<MyTraining[]> {
  return jsonOrThrow(await api('/api/safety/trainings/mine'), 'Error al obtener tus capacitaciones')
}

export async function createTraining(data: TrainingInput): Promise<SafetyTraining> {
  return jsonOrThrow(await api('/api/safety/trainings', send('POST', data)), 'Error al crear la capacitacion')
}

export async function updateTraining(id: string, data: TrainingInput): Promise<SafetyTraining> {
  return jsonOrThrow(await api(`/api/safety/trainings/${enc(id)}`, send('PUT', data)), 'Error al guardar la capacitacion')
}

export async function deleteTraining(id: string): Promise<void> {
  await okOrThrow(await api(`/api/safety/trainings/${enc(id)}`, { method: 'DELETE' }), 'Error al eliminar la capacitacion')
}

export async function fetchTrainingRecords(trainingId?: string): Promise<TrainingRecord[]> {
  const q = trainingId ? `?training_id=${enc(trainingId)}` : ''
  return jsonOrThrow(await api(`/api/safety/records${q}`), 'Error al obtener los registros')
}

export async function grantTraining(data: {
  training_id: string
  username: string
  granted_on?: string | null
  expires_on?: string | null
  notes?: string
}): Promise<TrainingRecord> {
  return jsonOrThrow(await api('/api/safety/records', send('POST', data)), 'Error al registrar la capacitacion')
}

export async function revokeTraining(trainingId: string, username: string): Promise<void> {
  await okOrThrow(await api(`/api/safety/records/${enc(trainingId)}/${enc(username)}`, { method: 'DELETE' }), 'Error al quitar el registro')
}

export async function fetchSheets(): Promise<SafetySheet[]> {
  return jsonOrThrow(await api('/api/safety/sheets'), 'Error al obtener las fichas')
}

export async function createSheet(data: SheetInput): Promise<SafetySheet> {
  return jsonOrThrow(await api('/api/safety/sheets', send('POST', data)), 'Error al crear la ficha')
}

export async function updateSheet(id: string, data: SheetInput): Promise<SafetySheet> {
  return jsonOrThrow(await api(`/api/safety/sheets/${enc(id)}`, send('PUT', data)), 'Error al guardar la ficha')
}

export async function deleteSheet(id: string): Promise<void> {
  await okOrThrow(await api(`/api/safety/sheets/${enc(id)}`, { method: 'DELETE' }), 'Error al eliminar la ficha')
}

export function sheetFileUrl(id: string): string {
  return `/api/safety/sheets/${enc(id)}/file`
}

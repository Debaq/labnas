import { api } from './client'

// --- Deteccion con IA en camaras ---

export interface DetectionSchedule {
  /** 0 = lunes ... 6 = domingo */
  days: number[]
  from: string
  to: string
}

export interface DetectionActions {
  photo: boolean
  telegram: boolean
  /** Segundos de video al detectar (0 = no grabar) */
  clip_s: number
  /** Impresion fallida: impresora que mira la camara */
  printer_id?: string | null
  /** Impresion fallida: pausarla cuando la falla es clara */
  pause_printer?: boolean
}

export interface DetectorInput {
  camera_id: string
  kind: 'personas' | 'objetos' | 'movimiento' | 'falla_impresion'
  classes: string[]
  /** Confianza minima (0..1); en movimiento, fraccion de la imagen que cambia */
  min_score: number
  interval_s: number
  schedule: DetectionSchedule | null
  actions: DetectionActions
  cooldown_s: number
  enabled: boolean
}

export interface DetectorRule extends DetectorInput {
  id: string
  camera: string
  active_now: boolean
  last_fire_at: string | null
  last_labels: string | null
  last_error: string | null
  /** Impresion fallida: nivel actual de 0 a 1 (null si no esta imprimiendo) */
  fail_level?: number | null
}

export interface DetectorsResponse {
  rules: DetectorRule[]
  model: { ready: boolean; downloading: boolean; error: string | null }
  kinds: [string, string][]
  /** [id en ingles, nombre en español] */
  classes: [string, string][]
  days: string[]
  /** Personas autorizadas a ver la deteccion (solo la recibe el admin) */
  watchers: string[]
}

export interface DetectionRow {
  id: string
  /** Regla que disparo */
  detector_id: string
  camera_id: string
  camera_name: string
  kind: string
  labels: string
  score: number
  at: string
  photo_id: string | null
  clip_id: string | null
  /** Chats de Telegram a los que llego (null = la regla no avisa, o deteccion antigua) */
  tg_sent: number | null
  /** Por que no llego el aviso */
  tg_error: string | null
}

export interface DetectionsFilter {
  camera?: string
  rule?: string
  /** `YYYY-MM-DD` */
  day?: string
  /** `at` de la ultima recibida, para cargar mas */
  before?: string
  limit?: number
}

async function failText(res: Response, fallback: string): Promise<never> {
  const t = await res.text().catch(() => '')
  throw new Error(t || fallback)
}

const json = (method: string, body: unknown): RequestInit => ({
  method,
  headers: { 'Content-Type': 'application/json' },
  body: JSON.stringify(body),
})

export async function fetchDetectors(): Promise<DetectorsResponse> {
  const res = await api('/api/cameras/detectors')
  if (!res.ok) return failText(res, 'Error al obtener las reglas de deteccion')
  return res.json()
}

export async function saveDetector(input: DetectorInput, id?: string): Promise<void> {
  const res = await api(id ? `/api/cameras/detectors/${id}` : '/api/cameras/detectors', json(id ? 'PUT' : 'POST', input))
  if (!res.ok) return failText(res, 'No se pudo guardar la regla')
}

export async function deleteDetector(id: string): Promise<void> {
  const res = await api(`/api/cameras/detectors/${id}`, { method: 'DELETE' })
  if (!res.ok) return failText(res, 'No se pudo borrar la regla')
}

/** Corre la regla (aunque no este guardada) sobre el cuadro actual */
export async function testDetector(input: DetectorInput): Promise<{ detections: { label: string; score: number }[]; image: string }> {
  const res = await api('/api/cameras/detectors/test', json('POST', input))
  if (!res.ok) return failText(res, 'No se pudo probar la deteccion')
  return res.json()
}

export async function prepareDetectionModel(): Promise<void> {
  const res = await api('/api/cameras/ai/model', { method: 'POST' })
  if (!res.ok) return failText(res, 'No se pudo descargar el modelo')
}

export async function fetchDetections(f: DetectionsFilter = {}): Promise<DetectionRow[]> {
  const p = new URLSearchParams()
  for (const [k, v] of Object.entries(f)) if (v !== undefined && v !== '') p.set(k, String(v))
  const res = await api(`/api/cameras/detections?${p}`)
  if (!res.ok) return failText(res, 'Error al obtener las detecciones')
  return res.json()
}

/** Quienes, ademas de los admins, pueden ver la deteccion (admin) */
export async function saveDetectionWatchers(users: string[]): Promise<void> {
  const res = await api('/api/cameras/detectors/watchers', json('PUT', { users }))
  if (!res.ok) return failText(res, 'No se pudo guardar quienes ven la deteccion')
}

// --- Obico (impresion fallida) ---

export interface ObicoStatus {
  url: string
  /** Direccion por la que Obico llega a LabNAS para bajar el cuadro */
  labnas_url: string
  has_token: boolean
  ready: boolean
  error: string | null
}

export async function fetchObico(): Promise<ObicoStatus> {
  const res = await api('/api/cameras/obico')
  if (!res.ok) return failText(res, 'Error al obtener la conexion con Obico')
  return res.json()
}

/** token: undefined = no cambiar, '' = quitar */
export async function saveObico(input: { url: string; labnas_url: string; token?: string }): Promise<ObicoStatus> {
  const res = await api('/api/cameras/obico', json('PUT', input))
  if (!res.ok) return failText(res, 'No se pudo guardar la conexion con Obico')
  return res.json()
}

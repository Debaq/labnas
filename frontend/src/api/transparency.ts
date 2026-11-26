import { api, jsonOrThrow } from './client'

// --- Transparencia y solicitudes de informacion (handlers/transparency) ---

export const PORTAL_SECTIONS = ['proyectos', 'montos', 'bienes', 'uso', 'informes'] as const
export type PortalSection = (typeof PORTAL_SECTIONS)[number]
export type SectionsState = Record<PortalSection, boolean>

export interface PortalProject { id: string; title: string; institution: string; funding_source: string; start_date: string; end_date: string | null; status: string }
export interface PortalCategory { category: string; approved: number; committed: number; spent: number }
export interface PortalAmounts { project_id: string; title: string; currency: string; approved: number; committed: number; spent: number; categories: PortalCategory[] }
export interface PortalAsset { name: string; inventory_number: string; cost: number; quantity: number; project: string }
export interface PortalUsage { machine: string; month: string; hours: number; sessions: number }
export interface PortalReport { id: string; title: string; date: string; project: string | null; file_name: string; size: number; sha256: string }

export interface Portal {
  lab: { lab_name: string; institution: string; logo_url: string }
  intro: string
  generated_at: string
  sections: PortalSection[]
  deadline_days: number
  proyectos?: PortalProject[]
  montos?: PortalAmounts[]
  bienes?: PortalAsset[]
  uso?: PortalUsage[]
  informes?: PortalReport[]
}

export interface TransparencyConfig {
  sections: SectionsState
  deadline_days: number
  intro: string
  managers: string[]
}

export interface PortalReportRow {
  id: string
  title: string
  date: string
  project_id: string | null
  project: string | null
  published: boolean
  file_name: string
  size: number
  sha256: string
  created_by: string
  created_at: string
  changed_by: string
  changed_at: string
}

export type RequestStatus = 'recibida' | 'en_curso' | 'respondida' | 'derivada' | 'rechazada'

export interface InfoRequest {
  id: string
  folio: string
  name: string
  email: string
  body: string
  status: RequestStatus
  assigned_to: string | null
  due_date: string
  /** Dias habiles que faltan (solo abiertas); negativo = atrasada */
  days_left: number | null
  response: string
  reason: string
  derived_to: string
  created_at: string
  responded_at: string | null
}

export interface InfoRequestEvent { at: string; username: string; status: RequestStatus; note: string; ledger_seq: number | null }

export interface RequestLookup {
  folio: string
  body: string
  status: RequestStatus
  created_at: string
  due_date: string
  days_left: number | null
  response: string
  reason: string
  derived_to: string
  responded_at: string | null
  history: { at: string; status: RequestStatus }[]
}

const P = '/api/transparency'
const json = (body: unknown, method = 'POST'): RequestInit => ({ method, headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) })

// Publico (sin sesion)
export const fetchPublicPortal = async (): Promise<Portal> => jsonOrThrow(await api(`${P}/public/portal`), 'No se pudo cargar el portal')
export const publicReportUrl = (id: string) => `${P}/public/informes/${encodeURIComponent(id)}`
export const submitInfoRequest = async (b: { name: string; email: string; body: string }): Promise<{ folio: string; code: string; due_date: string }> =>
  jsonOrThrow(await api(`${P}/public/solicitudes`, json(b)), 'No se pudo enviar la solicitud')
export const lookupInfoRequest = async (b: { folio: string; code: string }): Promise<RequestLookup> => {
  const res = await api(`${P}/public/solicitudes/consulta`, json(b))
  if (res.status === 404) throw new Error('No encontramos una solicitud con ese folio y codigo')
  return jsonOrThrow(res, 'No se pudo consultar')
}

// Gestion
export const fetchTransparencyConfig = async (): Promise<TransparencyConfig> => jsonOrThrow(await api(`${P}/config`), 'No se pudo cargar la configuracion')
export const saveTransparencyConfig = async (b: { sections: SectionsState; deadline_days: number; intro: string }): Promise<void> => {
  await jsonOrThrow(await api(`${P}/config`, json(b, 'PUT')), 'No se pudo guardar')
}
export const previewPortal = async (b: { sections: SectionsState; deadline_days: number; intro: string }): Promise<Portal> =>
  jsonOrThrow(await api(`${P}/preview`, json(b)), 'No se pudo armar la vista previa')
export const fetchPortalReports = async (): Promise<PortalReportRow[]> => jsonOrThrow(await api(`${P}/reports`), 'No se pudieron cargar los informes')
export const createPortalReport = async (b: { title: string; date: string; project_id: string | null; file: { name: string; data: string }; publish: boolean }): Promise<void> => {
  await jsonOrThrow(await api(`${P}/reports`, json(b)), 'No se pudo subir el informe')
}
export const setPortalReportPublished = async (id: string, published: boolean): Promise<void> => {
  await jsonOrThrow(await api(`${P}/reports/${id}/${published ? 'publish' : 'withdraw'}`, json({})), 'No se pudo cambiar el informe')
}
export const portalReportFileUrl = (id: string) => `${P}/reports/${encodeURIComponent(id)}/file`
export const fetchInfoRequests = async (status?: string): Promise<InfoRequest[]> =>
  jsonOrThrow(await api(`${P}/requests${status ? `?status=${status}` : ''}`), 'No se pudieron cargar las solicitudes')
export const fetchInfoRequest = async (id: string): Promise<{ request: InfoRequest; events: InfoRequestEvent[] }> =>
  jsonOrThrow(await api(`${P}/requests/${id}`), 'No se pudo cargar la solicitud')
export const updateInfoRequest = async (
  id: string,
  b: { status?: RequestStatus; assigned_to?: string; response?: string; reason?: string; derived_to?: string; note?: string },
): Promise<InfoRequest> => jsonOrThrow(await api(`${P}/requests/${id}`, json(b, 'PUT')), 'No se pudo guardar la solicitud')

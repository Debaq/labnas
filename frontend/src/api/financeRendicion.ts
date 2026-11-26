import { api, jsonOrThrow } from './client'
import { finJson, type FinFileRef } from './finance'

// --- Rendicion, expediente, manual, horas, anomalias (handlers/finance/rendicion.rs) ---

export interface RendDoc {
  id: string
  project_id: string
  project: string
  kind: string
  folio: string
  issuer_rut: string
  issuer_name: string
  issued_on: string | null
  net: number | null
  tax: number | null
  total: number | null
  budget_item_id: string | null
  budget_item: string
  purchase_id: string | null
  purchase_folio: number | null
  purchase_title: string
  file_id: string | null
  file_name: string | null
  file_sha256: string | null
}

export interface RendItem {
  id: string | null
  project_id: string
  project: string
  category: string
  name: string
  approved: number
  committed: number
  spent: number
  documented: number
  available: number
  over: boolean
}

export interface Rendicion {
  project_id: string | null
  project: string
  from: string | null
  to: string | null
  generated_at: string
  documents: RendDoc[]
  items: RendItem[]
  total_net: number
  total_tax: number
  total: number
  total_approved: number
  total_spent: number
}

export interface RendFilter {
  projectId: string | null
  from: string
  to: string
}

function rendQuery(f: RendFilter, extra?: Record<string, string>): string {
  const p = new URLSearchParams()
  if (f.projectId) p.set('project_id', f.projectId)
  if (f.from) p.set('from', f.from)
  if (f.to) p.set('to', f.to)
  for (const [k, v] of Object.entries(extra ?? {})) p.set(k, v)
  const s = p.toString()
  return s ? `?${s}` : ''
}

export async function fetchRendicion(f: RendFilter): Promise<Rendicion> {
  return jsonOrThrow(await api(`/api/finance/rendicion${rendQuery(f)}`), 'No se pudo cargar la rendicion')
}

/** URLs de descarga (el navegador manda la cookie de sesion) */
export const rendicionExportUrl = (f: RendFilter, format: 'xlsx' | 'csv' | 'zip') => `/api/finance/rendicion/export${rendQuery(f, { format })}`
export const rendicionPrintUrl = (f: RendFilter) => `/api/finance/rendicion/print${rendQuery(f)}`
export const expedienteUrl = (projectId: string) => `/api/finance/expediente?project_id=${encodeURIComponent(projectId)}`

// --- Anomalias ---

export type FindingKind = 'duplicado' | 'proveedor_nuevo' | 'fraccionamiento' | 'sin_factura' | 'funciones' | 'sobre_presupuesto'

export interface Finding {
  kind: FindingKind
  severity: 'alta' | 'media' | 'info'
  title: string
  detail: string
  project_id: string | null
  purchase_id: string | null
  document_id: string | null
}

export interface AnomalyReport {
  generated_at: string
  threshold: number
  findings: Finding[]
}

export async function fetchAnomalies(projectId: string | null): Promise<AnomalyReport> {
  const q = projectId ? `?project_id=${encodeURIComponent(projectId)}` : ''
  return jsonOrThrow(await api(`/api/finance/anomalies${q}`), 'No se pudo cargar el informe')
}

// --- Manual de procedimientos ---

export interface ManualVersionInfo {
  id: string
  version: number
  title: string
  summary: string
  sha256: string
  published_by: string
  published_at: string
  ledger_seq: number | null
  signatures: number
}

export interface ManualVersion extends ManualVersionInfo {
  body: string
}

export interface ManualView {
  current: ManualVersion | null
  versions: ManualVersionInfo[]
  my_signed_at: string | null
  pending_count: number
}

export interface ManualSignStatus {
  version: ManualVersionInfo | null
  required: string[]
  signed: { username: string; signed_at: string }[]
  pending: string[]
}

export async function fetchManual(): Promise<ManualView> {
  return jsonOrThrow(await api('/api/finance/manual'), 'No se pudo cargar el manual')
}

export async function fetchManualVersion(id: string): Promise<ManualVersion> {
  return jsonOrThrow(await api(`/api/finance/manual/versions/${encodeURIComponent(id)}`), 'No se pudo cargar la version')
}

export async function fetchManualSignatures(versionId?: string): Promise<ManualSignStatus> {
  const q = versionId ? `?version_id=${encodeURIComponent(versionId)}` : ''
  return jsonOrThrow(await api(`/api/finance/manual/signatures${q}`), 'No se pudo cargar las firmas')
}

export async function publishManual(body: { title?: string; body: string; summary: string }): Promise<ManualVersion> {
  return jsonOrThrow(await api('/api/finance/manual/versions', finJson(body)), 'No se pudo publicar')
}

export async function signManual(versionId: string): Promise<ManualSignStatus> {
  return jsonOrThrow(await api('/api/finance/manual/sign', finJson({ version_id: versionId })), 'No se pudo firmar')
}

// --- Horas por persona y proyecto ---

export type HourStatus = 'pendiente' | 'aprobada' | 'rechazada' | 'anulada'

export interface HourRow {
  id: string
  project_id: string
  project: string
  username: string
  month: string
  hours: number
  amount: number
  budget_item_id: string | null
  notes: string
  file: FinFileRef | null
  status: HourStatus
  created_by: string
  created_at: string
  decided_by: string | null
  decided_at: string | null
  decision_note: string
}

export interface HourSummaryRow {
  project_id: string
  project: string
  username: string
  approved_hours: number
  approved_amount: number
  pending_hours: number
  pending_amount: number
}

export async function fetchHours(projectId: string | null): Promise<HourRow[]> {
  const q = projectId ? `?project_id=${encodeURIComponent(projectId)}` : ''
  return jsonOrThrow(await api(`/api/finance/hours${q}`), 'No se pudo cargar las horas')
}

export async function fetchHoursSummary(projectId: string | null): Promise<HourSummaryRow[]> {
  const q = projectId ? `?project_id=${encodeURIComponent(projectId)}` : ''
  return jsonOrThrow(await api(`/api/finance/hours/summary${q}`), 'No se pudo cargar el resumen')
}

export interface HoursInput {
  project_id: string
  username?: string
  month: string
  hours: number
  amount: number
  budget_item_id?: string | null
  notes?: string
  file?: { name: string; data: string } | null
}

export async function createHours(input: HoursInput): Promise<HourRow> {
  return jsonOrThrow(await api('/api/finance/hours', finJson(input)), 'No se pudo registrar')
}

export async function decideHours(id: string, action: 'approve' | 'reject' | 'annul', note = ''): Promise<HourRow> {
  return jsonOrThrow(await api(`/api/finance/hours/${encodeURIComponent(id)}/${action}`, finJson({ note })), 'No se pudo guardar la decision')
}

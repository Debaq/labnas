import { api, jsonOrThrow } from './client'
import type { FinFileRef } from './finance'

// --- Viajes y viaticos (handlers/finance/travel.rs) ---

export type TripStatus = 'solicitud' | 'aprobado' | 'rechazado' | 'anulado' | 'cerrado'
export type ReceiptKind = 'pasaje' | 'alojamiento' | 'viatico' | 'otro' | 'informe'

export interface TripReceipt {
  id: string
  kind: ReceiptKind
  description: string
  amount: number
  file: FinFileRef | null
  added_by: string
  added_at: string
}

export interface TripStep { step: string; username: string; at: string; note: string; ledger_seq: number | null }

export interface Trip {
  id: string
  folio: number
  project_id: string
  project_title: string
  budget_item_id: string | null
  traveler: string
  traveler_user: string | null
  destination: string
  start_date: string
  end_date: string
  purpose: string
  est_transport: number
  est_lodging: number
  est_per_diem: number
  est_total: number
  status: TripStatus
  requested_by: string
  decided_by: string | null
  decided_at: string | null
  decision_note: string
  report_text: string
  closed_by: string | null
  closed_at: string | null
  /** Suma de comprobantes */
  spent: number
  created_at: string
  receipts?: TripReceipt[]
  steps?: TripStep[]
}

export interface TripInput {
  project_id: string
  budget_item_id?: string | null
  traveler: string
  traveler_user?: string | null
  destination: string
  start_date: string
  end_date: string
  purpose: string
  est_transport: number
  est_lodging: number
  est_per_diem: number
}

const T = '/api/finance/trips'
const json = (body: unknown, method = 'POST'): RequestInit => ({ method, headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) })

export const fetchTrips = async (projectId?: string | null): Promise<Trip[]> =>
  jsonOrThrow(await api(`${T}${projectId ? `?project_id=${encodeURIComponent(projectId)}` : ''}`), 'No se pudieron cargar los viajes')
export const fetchTrip = async (id: string): Promise<Trip> => jsonOrThrow(await api(`${T}/${id}`), 'No se pudo cargar el viaje')
export const createTrip = async (b: TripInput): Promise<Trip> => jsonOrThrow(await api(T, json(b)), 'No se pudo pedir el viaje')
export const updateTrip = async (id: string, b: TripInput): Promise<Trip> => jsonOrThrow(await api(`${T}/${id}`, json(b, 'PUT')), 'No se pudo guardar el viaje')
export const approveTrip = async (id: string, note: string): Promise<Trip> => jsonOrThrow(await api(`${T}/${id}/approve`, json({ note })), 'No se pudo aprobar')
export const rejectTrip = async (id: string, note: string): Promise<Trip> => jsonOrThrow(await api(`${T}/${id}/reject`, json({ note })), 'No se pudo rechazar')
export const cancelTrip = async (id: string, reason: string): Promise<Trip> => jsonOrThrow(await api(`${T}/${id}/cancel`, json({ reason })), 'No se pudo anular')
export const addTripReceipt = async (
  id: string,
  b: { kind: ReceiptKind; description: string; amount: number; file?: { name: string; data: string } | null },
): Promise<Trip> => jsonOrThrow(await api(`${T}/${id}/receipts`, json(b)), 'No se pudo agregar el comprobante')
export const removeTripReceipt = async (id: string, rid: string): Promise<Trip> =>
  jsonOrThrow(await api(`${T}/${id}/receipts/${rid}`, { method: 'DELETE' }), 'No se pudo quitar el comprobante')
export const saveTripReport = async (id: string, text: string): Promise<Trip> => jsonOrThrow(await api(`${T}/${id}/report`, json({ text }, 'PUT')), 'No se pudo guardar el informe')
export const closeTrip = async (id: string): Promise<Trip> => jsonOrThrow(await api(`${T}/${id}/close`, json({})), 'No se pudo cerrar el viaje')

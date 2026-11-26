import { api } from './client'
import { finJson, finJsonOrThrow, type FinFileRef } from './finance'

// --- Compras (handlers/finance/purchases.rs) ---

export type PurchaseStatus =
  | 'solicitud' | 'cotizacion' | 'aprobada' | 'orden' | 'recibida' | 'facturada' | 'pagada' | 'rendida' | 'rechazada' | 'anulada'

export interface Purchase {
  id: string
  folio: number
  project_id: string
  project_title: string | null
  budget_item_id: string | null
  budget_item_name: string | null
  title: string
  justification: string
  estimated_amount: number
  final_amount: number | null
  status: PurchaseStatus
  requested_by: string
  approved_by: string | null
  received_by: string | null
  chosen_quote_id: string | null
  choice_reason: string
  created_at: string
  updated_at: string
  quotes_count: number
  chosen_supplier: string | null
}

export interface PurchaseStep {
  id: string
  step: string
  username: string
  at: string
  note: string
  passkey: boolean
  ledger_seq: number | null
  ledger_hash: string | null
}

export interface PurchaseQuote {
  id: string
  supplier: string
  supplier_rut: string
  amount: number
  valid_until: string | null
  file: FinFileRef | null
  notes: string
  created_by: string
  created_at: string
  cheapest: boolean
}

export interface PurchaseDoc {
  id: string
  kind: string
  folio: string
  issuer_rut: string
  issuer_name: string
  issued_on: string | null
  net_amount: number | null
  tax_amount: number | null
  total_amount: number | null
  file: FinFileRef | null
  extracted_by: 'sii_xml' | 'ia' | 'manual'
  confirmed_by: string | null
  created_by: string
  created_at: string
}

export interface PurchaseReception {
  id: string
  received_by: string
  received_at: string
  conforming: boolean
  notes: string
  photo: FinFileRef | null
}

export interface PurchaseDeclaration {
  id: string
  username: string
  supplier: string
  has_conflict: boolean
  detail: string
  at: string
  ledger_seq: number | null
}

export interface PurchaseSettings {
  min_quotes: number
  quote_threshold: number
  require_passkey: boolean
}

export interface PurchaseDetail {
  purchase: Purchase
  steps: PurchaseStep[]
  quotes: PurchaseQuote[]
  documents: PurchaseDoc[]
  receptions: PurchaseReception[]
  declarations: PurchaseDeclaration[]
  settings: PurchaseSettings
  required_quotes: number
  budget: { item_name: string; approved: number; committed: number; spent: number; available: number } | null
  warnings: string[]
}

export interface FileIn { name: string; data: string }

export interface DocFields {
  kind?: string
  folio?: string
  issuer_rut?: string
  issuer_name?: string
  issued_on?: string
  net?: number | null
  tax?: number | null
  total?: number | null
}

const BASE = '/api/finance/purchases'

async function act(path: string, body: unknown, fallback: string): Promise<PurchaseDetail> {
  return finJsonOrThrow(await api(`${BASE}/${path}`, finJson(body ?? {})), fallback)
}

export async function fetchPurchases(opts: { project?: string | null; status?: string } = {}): Promise<Purchase[]> {
  const q = new URLSearchParams()
  if (opts.project) q.set('project', opts.project)
  if (opts.status) q.set('status', opts.status)
  return finJsonOrThrow(await api(`${BASE}?${q}`), 'No se pudieron cargar las compras')
}

export async function fetchPurchase(id: string): Promise<PurchaseDetail> {
  return finJsonOrThrow(await api(`${BASE}/${id}`), 'No se pudo cargar la compra')
}

export async function createPurchase(body: {
  project_id: string
  budget_item_id: string | null
  title: string
  justification: string
  estimated_amount: number
}): Promise<PurchaseDetail> {
  return finJsonOrThrow(await api(BASE, finJson(body)), 'No se pudo crear la solicitud')
}

export const startQuoting = (id: string) => act(`${id}/quoting`, {}, 'No se pudo pasar a cotizacion')

export const addPurchaseQuote = (id: string, body: {
  supplier: string; supplier_rut: string; amount: number; valid_until: string | null; notes: string; file: FileIn
}) => act(`${id}/quotes`, body, 'No se pudo agregar la cotizacion')

export interface Assertion { id: string; clientDataJSON: string; authenticatorData: string; signature: string }

export const approvePurchase = (id: string, body: {
  quote_id: string | null
  reason: string
  conflict: { has_conflict: boolean; detail: string }
  passkey?: Assertion
}) => act(`${id}/approve`, body, 'No se pudo aprobar')

export const rejectPurchase = (id: string, reason: string) => act(`${id}/reject`, { reason }, 'No se pudo rechazar')
export const cancelPurchase = (id: string, reason: string) => act(`${id}/cancel`, { reason }, 'No se pudo anular')
export const orderPurchase = (id: string, note: string) => act(`${id}/order`, { note }, 'No se pudo emitir la orden')

export const receivePurchase = (id: string, body: {
  conforming: boolean
  notes: string
  photo: FileIn | null
  inventory: { name: string | null; quantity: number } | null
}) => act(`${id}/receive`, body, 'No se pudo registrar la recepcion')

export const uploadInvoice = (id: string, file: FileIn, fields: DocFields = {}) =>
  act(`${id}/invoice`, { file, ...fields }, 'No se pudo subir la factura')

export const confirmInvoice = (id: string, doc: string, fields: DocFields) =>
  act(`${id}/invoice/${doc}/confirm`, fields, 'No se pudo confirmar la factura')

export const payPurchase = (id: string, paid_on: string | null, reference: string) =>
  act(`${id}/pay`, { paid_on, reference }, 'No se pudo registrar el pago')

export const settlePurchase = (id: string, note: string) => act(`${id}/settle`, { note }, 'No se pudo rendir')

export async function fetchPurchaseSettings(): Promise<PurchaseSettings> {
  return finJsonOrThrow(await api(`${BASE}/settings`), 'No se pudieron cargar los ajustes')
}

export async function savePurchaseSettings(s: PurchaseSettings): Promise<PurchaseSettings> {
  return finJsonOrThrow(await api(`${BASE}/settings`, finJson(s, 'PUT')), 'No se pudieron guardar los ajustes')
}

// --- Firma con passkey ---

function toB64url(buf: ArrayBuffer): string {
  let s = ''
  for (const b of new Uint8Array(buf)) s += String.fromCharCode(b)
  return btoa(s).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '')
}

function fromB64url(s: string): ArrayBuffer {
  const b64 = s.replace(/-/g, '+').replace(/_/g, '/') + '==='.slice((s.length + 3) % 4)
  const bin = atob(b64)
  const out = new Uint8Array(bin.length)
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i)
  return out.buffer
}

/** Pide al navegador firmar la aprobacion de esta compra y monto con una passkey del usuario */
export async function signApproval(id: string, quoteId: string | null): Promise<Assertion> {
  const opts = await finJsonOrThrow<{
    challenge: string; rpId: string; timeout: number; userVerification: UserVerificationRequirement
    allowCredentials: { type: 'public-key'; id: string }[]
  }>(await api(`${BASE}/${id}/passkey/options`, finJson({ quote_id: quoteId })), 'No se pudo iniciar la firma')
  const cred = (await navigator.credentials.get({
    publicKey: {
      ...opts,
      challenge: fromB64url(opts.challenge),
      allowCredentials: opts.allowCredentials.map((c) => ({ type: c.type, id: fromB64url(c.id) })),
    },
  })) as PublicKeyCredential | null
  if (!cred) throw new Error('Firma cancelada')
  const r = cred.response as AuthenticatorAssertionResponse
  return {
    id: toB64url(cred.rawId),
    clientDataJSON: toB64url(r.clientDataJSON),
    authenticatorData: toB64url(r.authenticatorData),
    signature: toB64url(r.signature),
  }
}

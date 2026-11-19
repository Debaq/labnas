import { api } from './client'

// --- Email ---

export interface EmailMessage {
  uid: number
  from: string
  subject: string
  date: string
  body_preview: string
  ai_classification: string | null
  ai_summary: string | null
  ai_action: string | null
  filter_label: string | null
  filter_action: string | null
  processed: boolean
  task_created: boolean
  fetched_at: string
}

export interface EmailFilter {
  pattern: string
  action: 'prioritario' | 'normal' | 'silencioso' | 'ignorar'
  label: string
  auto_tag: string | null
}

export async function configureEmailAccount(data: { host: string; port: number; protocol: 'imap' | 'pop3'; email: string; password: string }): Promise<string> {
  const res = await api('/api/email/account', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(data),
  })
  if (!res.ok) { const t = await res.text(); throw new Error(t) }
  return res.text()
}

export async function deleteEmailAccount(): Promise<void> {
  const res = await api('/api/email/account', { method: 'DELETE' })
  if (!res.ok) throw new Error('Error al eliminar cuenta')
}

export async function fetchInbox(): Promise<EmailMessage[]> {
  const res = await api('/api/email/inbox')
  if (!res.ok) throw new Error('Error al obtener bandeja')
  return res.json()
}

export async function checkEmailNow(): Promise<string> {
  const res = await api('/api/email/check', { method: 'POST' })
  if (!res.ok) { const t = await res.text(); throw new Error(t) }
  return res.text()
}

export async function classifyEmail(uid: number): Promise<EmailMessage> {
  const res = await api(`/api/email/classify/${uid}`, { method: 'POST' })
  if (!res.ok) throw new Error('Error al clasificar')
  return res.json()
}

export async function emailToTask(uid: number): Promise<string> {
  const res = await api(`/api/email/to-task/${uid}`, { method: 'POST' })
  if (!res.ok) { const t = await res.text(); throw new Error(t) }
  return res.text()
}

export interface Proposal {
  id: string
  kind: 'evento' | 'tarea'
  source: 'ia' | 'ics'
  title: string
  description: string
  date: string | null
  time: string | null
  end_time: string | null
  location: string | null
  status: string
  email_uid: number | null
  email_subject: string | null
  email_from: string | null
  created_at: string
}

export interface ProposalEdits {
  title?: string
  date?: string
  time?: string
  end_time?: string
  location?: string
  description?: string
}

export interface AiConfig {
  base_url: string
  model: string
  has_key: boolean
  enabled: boolean
}

async function failText(res: Response, fallback: string): Promise<never> {
  const t = await res.text().catch(() => '')
  throw new Error(t || fallback)
}

export async function fetchProposals(status: 'pendiente' | 'aprobada' | 'descartada' = 'pendiente'): Promise<Proposal[]> {
  const res = await api(`/api/email/proposals?status=${status}`)
  if (!res.ok) return failText(res, 'Error al obtener propuestas')
  return res.json()
}

export async function approveProposal(id: string, edits?: ProposalEdits): Promise<{ kind: 'evento' | 'tarea'; id: string; message: string }> {
  const res = await api(`/api/email/proposals/${encodeURIComponent(id)}/approve`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(edits ?? {}),
  })
  if (!res.ok) return failText(res, 'Error al aprobar')
  return res.json()
}

export async function dismissProposal(id: string): Promise<void> {
  const res = await api(`/api/email/proposals/${encodeURIComponent(id)}/dismiss`, { method: 'POST' })
  if (!res.ok) return failText(res, 'Error al descartar')
}

export async function fetchAiConfig(): Promise<AiConfig> {
  const res = await api('/api/ai/config')
  if (!res.ok) return failText(res, 'Error al obtener configuracion de IA')
  return res.json()
}

export async function saveAiConfig(cfg: { base_url: string; model: string; api_key?: string }): Promise<AiConfig> {
  const res = await api('/api/ai/config', {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(cfg),
  })
  if (!res.ok) return failText(res, 'Error al guardar configuracion')
  return res.json()
}

/** Modelos que ofrece el proveedor. Sin `api_key` usa la guardada (solo para la misma URL) */
export async function fetchAiModels(base_url: string, api_key?: string): Promise<string[]> {
  const res = await api('/api/ai/models', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ base_url, api_key: api_key || undefined }),
  })
  if (!res.ok) return failText(res, 'No se pudo obtener la lista de modelos')
  return res.json()
}

export async function testAi(): Promise<{ ok: boolean; model: string; ms: number }> {
  const res = await api('/api/ai/test', { method: 'POST' })
  if (!res.ok) return failText(res, 'Error al probar conexion')
  return res.json()
}

export async function fetchEmailFilters(): Promise<EmailFilter[]> {
  const res = await api('/api/email/filters')
  if (!res.ok) throw new Error('Error al obtener filtros')
  return res.json()
}

export async function addEmailFilter(filter: { pattern: string; action: string; label: string; auto_tag?: string }): Promise<void> {
  const res = await api('/api/email/filters', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(filter),
  })
  if (!res.ok) { const t = await res.text(); throw new Error(t) }
}

export async function deleteEmailFilter(pattern: string): Promise<void> {
  const res = await api(`/api/email/filters/${encodeURIComponent(pattern)}`, { method: 'DELETE' })
  if (!res.ok) throw new Error('Error al eliminar filtro')
}

// --- Cuenta y OAuth de Microsoft ---

export interface EmailAccountInfo {
  email: string
  host: string
  port: number
  protocol: 'imap' | 'pop3'
  auth: 'password' | 'microsoft'
  last_error: string | null
  last_check: string | null
}

/** null si el usuario no tiene cuenta */
export async function fetchEmailAccount(): Promise<EmailAccountInfo | null> {
  const res = await api('/api/email/account')
  if (res.status === 404) return null
  if (!res.ok) return failText(res, 'Error al obtener la cuenta')
  return res.json()
}

export interface MsOAuthConfig {
  configured: boolean
  client_id: string
  tenant: string
}

export interface MsFlowStatus {
  status: 'ninguno' | 'pendiente' | 'conectado' | 'error'
  message: string
  user_code?: string | null
  verification_uri?: string | null
}

export async function fetchMsOAuthConfig(): Promise<MsOAuthConfig> {
  const res = await api('/api/email/oauth/microsoft/config')
  if (!res.ok) return failText(res, 'Error al obtener la configuracion de Microsoft')
  return res.json()
}

export async function saveMsOAuthConfig(cfg: { client_id: string; tenant?: string }): Promise<MsOAuthConfig> {
  const res = await api('/api/email/oauth/microsoft/config', {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(cfg),
  })
  if (!res.ok) return failText(res, 'Error al guardar')
  return res.json()
}

export async function startMsOAuth(email: string): Promise<MsFlowStatus> {
  const res = await api('/api/email/oauth/microsoft/start', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ email }),
  })
  if (!res.ok) return failText(res, 'No se pudo iniciar la conexion con Microsoft')
  return res.json()
}

export async function fetchMsOAuthStatus(): Promise<MsFlowStatus> {
  const res = await api('/api/email/oauth/microsoft/status')
  if (!res.ok) return failText(res, 'Error al consultar el estado')
  return res.json()
}

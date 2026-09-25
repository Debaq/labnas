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

export async function setGroqKey(key: string): Promise<string> {
  const res = await api('/api/email/groq-key', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ key }),
  })
  if (!res.ok) throw new Error('Error al configurar key')
  return res.text()
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

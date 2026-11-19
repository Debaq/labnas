import { api } from './client'

// --- Bitacora inalterable (admin) ---

export interface LedgerEntry {
  seq: number
  timestamp: string
  username: string
  kind: string
  subject: string
  annuls: number | null
  /** JSON exacto que entra en la huella */
  data: string
  prev_hash: string
  hash: string
  annulled_by: number | null
}

export interface LedgerVerify {
  ok: boolean
  count: number
  head_seq: number | null
  head_hash: string
  broken_at: number | null
  reason: string | null
  last_anchor: { seq: number; hash: string; at: string; via: 'telegram' | 'texto' } | null
}

export interface LedgerFileCheck {
  path: string
  sha256: string
  size: number
  status: 'igual' | 'cambiado' | 'no_existe' | 'sin_acceso'
  current_sha256: string | null
}

async function failText(res: Response, fallback: string): Promise<never> {
  const t = await res.text().catch(() => '')
  throw new Error(t || fallback)
}

export async function fetchLedger(params: { q?: string; kind?: string; before_seq?: number; limit?: number }): Promise<LedgerEntry[]> {
  const qs = new URLSearchParams()
  if (params.q) qs.set('q', params.q)
  if (params.kind) qs.set('kind', params.kind)
  if (params.before_seq) qs.set('before_seq', String(params.before_seq))
  if (params.limit) qs.set('limit', String(params.limit))
  const res = await api(`/api/ledger?${qs}`)
  if (!res.ok) return failText(res, 'Error al obtener la bitácora')
  return res.json()
}

export async function verifyLedger(): Promise<LedgerVerify> {
  const res = await api('/api/ledger/verify')
  if (!res.ok) return failText(res, 'Error al verificar la bitácora')
  return res.json()
}

export async function addLedgerNote(note: { title: string; text: string; subject: string; files: string[] }): Promise<LedgerEntry> {
  const res = await api('/api/ledger/notes', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(note),
  })
  if (!res.ok) return failText(res, 'No se pudo registrar la nota')
  return res.json()
}

export async function annulLedgerEntry(seq: number, reason: string): Promise<LedgerEntry> {
  const res = await api(`/api/ledger/${seq}/annul`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ reason }),
  })
  if (!res.ok) return failText(res, 'No se pudo anular')
  return res.json()
}

export async function checkLedgerFiles(seq: number): Promise<LedgerFileCheck[]> {
  const res = await api(`/api/ledger/${seq}/files`)
  if (!res.ok) return failText(res, 'No se pudieron revisar los archivos')
  return res.json()
}

/** Envia la huella final a los admins por Telegram y devuelve el texto para guardarlo afuera */
export async function anchorLedger(): Promise<{ text: string; telegram_sent: number }> {
  const res = await api('/api/ledger/anchor', { method: 'POST' })
  if (!res.ok) return failText(res, 'No se pudo anclar la bitácora')
  return res.json()
}

/** Descarga la bitacora completa (JSONL); necesita la sesion, por eso no es un <a href> */
export async function downloadLedger(): Promise<void> {
  const res = await api('/api/ledger/export')
  if (!res.ok) return failText(res, 'Error al exportar la bitácora')
  const name = /filename="([^"]+)"/.exec(res.headers.get('Content-Disposition') ?? '')?.[1] ?? 'bitacora-labnas.jsonl'
  const url = URL.createObjectURL(await res.blob())
  const a = document.createElement('a')
  a.href = url
  a.download = name
  a.click()
  setTimeout(() => URL.revokeObjectURL(url), 10_000)
}

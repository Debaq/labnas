import { api } from './client'

export interface BridgeStatus {
  connected: boolean
  last_ok: string | null
  last_error: string | null
  served: number
  remote: {
    version?: string
    php?: string
    server?: string
    limits?: { post_max_size: number; upload_max_filesize: number; max_execution_time: number; memory_limit: number }
  } | null
}

export interface BridgeInfo {
  linked: boolean
  url: string | null
  enabled: boolean
  status: BridgeStatus
}

async function failText(res: Response, fallback: string): Promise<never> {
  const t = await res.text().catch(() => '')
  throw new Error(t || fallback)
}

export async function fetchBridge(): Promise<BridgeInfo> {
  const res = await api('/api/bridge')
  if (!res.ok) return failText(res, 'Error al obtener el estado del puente')
  return res.json()
}

export async function linkBridge(url: string, code: string): Promise<BridgeInfo> {
  const res = await api('/api/bridge/link', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ url, code }),
  })
  if (!res.ok) return failText(res, 'No se pudo enlazar')
  return res.json()
}

export async function setBridgeEnabled(enabled: boolean): Promise<BridgeInfo> {
  const res = await api('/api/bridge/enabled', {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ enabled }),
  })
  if (!res.ok) return failText(res, 'Error al cambiar el puente')
  return res.json()
}

export async function unlinkBridge(): Promise<void> {
  const res = await api('/api/bridge', { method: 'DELETE' })
  if (!res.ok) return failText(res, 'Error al desenlazar')
}

/** Descarga el zip del conector (necesita el token, por eso no es un <a href>) */
export async function downloadBridgePackage(): Promise<void> {
  const res = await api('/api/bridge/package')
  if (!res.ok) return failText(res, 'Error al generar el paquete')
  const name = /filename="([^"]+)"/.exec(res.headers.get('Content-Disposition') ?? '')?.[1] ?? 'labnas-conector.zip'
  const url = URL.createObjectURL(await res.blob())
  const a = document.createElement('a')
  a.href = url
  a.download = name
  a.click()
  setTimeout(() => URL.revokeObjectURL(url), 10_000)
}

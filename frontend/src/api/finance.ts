import { api, jsonOrThrow } from './client'

// --- Piezas comunes de Finanzas (backend/src/finance.rs) ---

export const BUDGET_CATEGORIES = ['personal', 'operacion', 'equipos', 'viajes', 'otro'] as const
export type BudgetCategory = (typeof BUDGET_CATEGORIES)[number]

export interface FinFileRef {
  id: string
  name: string
  mime: string
  size: number
  sha256: string
}

/** URL de descarga de un respaldo */
export function finFileUrl(id: string): string {
  return `/api/finance/files/${encodeURIComponent(id)}`
}

/** Lee un File del navegador como data URL (para subir respaldos en JSON) */
export function fileToDataUrl(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const r = new FileReader()
    r.onload = () => resolve(String(r.result))
    r.onerror = () => reject(new Error('No se pudo leer el archivo'))
    r.readAsDataURL(file)
  })
}

export const finJson = (body: unknown, method = 'POST'): RequestInit => ({
  method,
  headers: { 'Content-Type': 'application/json' },
  body: JSON.stringify(body),
})

export { api as finApi, jsonOrThrow as finJsonOrThrow }

import { api } from './client'

// --- Inventory ---

export async function fetchInventoryCategories(): Promise<import('../types').InventoryCategory[]> {
  const res = await api('/api/inventory/categories')
  if (!res.ok) throw new Error('Error al obtener categorias')
  return res.json()
}

export async function createInventoryCategory(data: { name: string; icon?: string; description?: string }): Promise<import('../types').InventoryCategory> {
  const res = await api('/api/inventory/categories', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(data) })
  if (!res.ok) throw new Error('Error al crear categoria')
  return res.json()
}

export async function updateInventoryCategory(id: string, data: { name: string; icon?: string; description?: string }): Promise<import('../types').InventoryCategory> {
  const res = await api(`/api/inventory/categories/${id}`, { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(data) })
  if (!res.ok) throw new Error('Error al actualizar categoria')
  return res.json()
}

export async function deleteInventoryCategory(id: string): Promise<void> {
  const res = await api(`/api/inventory/categories/${id}`, { method: 'DELETE' })
  if (!res.ok) throw new Error('Error al eliminar categoria')
}

export async function fetchInventoryItems(): Promise<import('../types').InventoryItem[]> {
  const res = await api('/api/inventory/items')
  if (!res.ok) throw new Error('Error al obtener items')
  return res.json()
}

export async function createInventoryItem(data: Partial<import('../types').InventoryItem>): Promise<import('../types').InventoryItem> {
  const res = await api('/api/inventory/items', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(data) })
  if (!res.ok) throw new Error('Error al crear item')
  return res.json()
}

export async function updateInventoryItem(id: string, data: Partial<import('../types').InventoryItem>): Promise<import('../types').InventoryItem> {
  const res = await api(`/api/inventory/items/${id}`, { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(data) })
  if (!res.ok) throw new Error('Error al actualizar item')
  return res.json()
}

export async function deleteInventoryItem(id: string): Promise<void> {
  const res = await api(`/api/inventory/items/${id}`, { method: 'DELETE' })
  if (!res.ok) throw new Error('Error al eliminar item')
}

export async function fetchPrintHistory(): Promise<import('../types').PrintHistoryEntry[]> {
  const res = await api('/api/inventory/print-history')
  if (!res.ok) throw new Error('Error al obtener historial')
  return res.json()
}

export async function addPrintHistory(data: Partial<import('../types').PrintHistoryEntry>): Promise<import('../types').PrintHistoryEntry> {
  const res = await api('/api/inventory/print-history', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(data) })
  if (!res.ok) throw new Error('Error al registrar impresion')
  return res.json()
}

export async function deletePrintHistory(id: string): Promise<void> {
  const res = await api(`/api/inventory/print-history/${id}`, { method: 'DELETE' })
  if (!res.ok) throw new Error('Error al eliminar entrada')
}

// --- Prestamos, stock minimo y QR (backend/src/handlers/inventory_loans.rs) ---

export async function fetchLoans(q: { open?: boolean; mine?: boolean; item?: string } = {}): Promise<import('../types').InventoryLoan[]> {
  const p = new URLSearchParams()
  if (q.open) p.set('open', 'true')
  if (q.mine) p.set('mine', 'true')
  if (q.item) p.set('item', q.item)
  const res = await api(`/api/inventory/loans?${p}`)
  if (!res.ok) throw new Error('Error al leer los prestamos')
  return res.json()
}

export async function createLoan(itemId: string, data: { borrower?: string; quantity: number; due_on: string; note?: string }): Promise<import('../types').InventoryLoan> {
  const res = await api(`/api/inventory/items/${encodeURIComponent(itemId)}/loans`, {
    method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(data),
  })
  if (!res.ok) throw new Error((await res.text()) || 'Error al prestar')
  return res.json()
}

export async function returnLoan(id: string, note = ''): Promise<import('../types').InventoryLoan> {
  const res = await api(`/api/inventory/loans/${encodeURIComponent(id)}/return`, {
    method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ note }),
  })
  if (!res.ok) throw new Error((await res.text()) || 'Error al devolver')
  return res.json()
}

export async function fetchInventoryQr(id: string): Promise<{ url: string; svg: string }> {
  const res = await api(`/api/inventory/items/${encodeURIComponent(id)}/qr?base=${encodeURIComponent(window.location.origin)}`)
  if (!res.ok) throw new Error((await res.text()) || 'Error al generar el QR')
  return res.json()
}

// --- Retazos (backend/src/handlers/offcuts.rs) ---

export interface Offcut {
  id: string
  material: string
  thickness_mm: number | null
  width_mm: number
  height_mm: number
  location: string
  notes: string
  has_photo: boolean
  added_by: string
  added_at: string
  taken_by: string | null
  taken_at: string | null
}

export async function fetchOffcuts(q: { material?: string; thickness?: number | null; min_w?: number | null; min_h?: number | null } = {}): Promise<Offcut[]> {
  const p = new URLSearchParams()
  if (q.material) p.set('material', q.material)
  if (q.thickness) p.set('thickness', String(q.thickness))
  if (q.min_w) p.set('min_w', String(q.min_w))
  if (q.min_h) p.set('min_h', String(q.min_h))
  const res = await api(`/api/inventory/offcuts?${p}`)
  if (!res.ok) throw new Error('Error al leer los retazos')
  return res.json()
}

export async function createOffcut(data: { material: string; thickness_mm: number | null; width_mm: number; height_mm: number; location: string; notes: string; photo?: string }): Promise<Offcut> {
  const res = await api('/api/inventory/offcuts', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(data) })
  if (!res.ok) throw new Error((await res.text()) || 'Error al guardar el retazo')
  return res.json()
}

export async function takeOffcut(id: string): Promise<Offcut> {
  const res = await api(`/api/inventory/offcuts/${encodeURIComponent(id)}/take`, { method: 'POST' })
  if (!res.ok) throw new Error((await res.text()) || 'Error')
  return res.json()
}

export async function deleteOffcut(id: string): Promise<void> {
  const res = await api(`/api/inventory/offcuts/${encodeURIComponent(id)}`, { method: 'DELETE' })
  if (!res.ok) throw new Error((await res.text()) || 'Error al borrar')
}

export function offcutPhotoUrl(id: string): string {
  return `/api/inventory/offcuts/${encodeURIComponent(id)}/photo`
}

// --- Reciclaje (backend/src/handlers/recycling.rs) ---

export type RecyclingKind = 'triturado' | 'recuperado' | 'desecho'

export interface RecyclingEntry {
  id: string
  date: string
  material: string
  kind: RecyclingKind
  kg: number
  notes: string
  created_by: string
}

export interface RecyclingSummary {
  entries: RecyclingEntry[]
  by_month: [string, RecyclingKind, number][]
  by_material: [string, RecyclingKind, number][]
  total_triturado: number
  total_recuperado: number
  total_desecho: number
}

export async function fetchRecycling(): Promise<RecyclingSummary> {
  const res = await api('/api/inventory/recycling')
  if (!res.ok) throw new Error('Error al leer el reciclaje')
  return res.json()
}

export async function addRecycling(data: { date: string; material: string; kind: RecyclingKind; kg: number; notes: string }): Promise<RecyclingEntry> {
  const res = await api('/api/inventory/recycling', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(data) })
  if (!res.ok) throw new Error((await res.text()) || 'Error al guardar')
  return res.json()
}

export async function deleteRecycling(id: string): Promise<void> {
  const res = await api(`/api/inventory/recycling/${encodeURIComponent(id)}`, { method: 'DELETE' })
  if (!res.ok) throw new Error((await res.text()) || 'Error al borrar')
}

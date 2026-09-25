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

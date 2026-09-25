import { api } from './client'

// ═══════════════════════════════════════
// Modules
// ═══════════════════════════════════════

export async function fetchModules(): Promise<import('../types').ModuleInfo[]> {
  const res = await api('/api/modules')
  if (!res.ok) throw new Error('Error al obtener modulos')
  return res.json()
}

export async function toggleModule(id: string, enabled: boolean): Promise<void> {
  const res = await api(`/api/modules/${id}`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ enabled }),
  })
  if (!res.ok) throw new Error('Error al cambiar estado del modulo')
}

export async function reorderModules(order: { id: string; order: number }[]): Promise<void> {
  const res = await api('/api/modules/order', {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(order),
  })
  if (!res.ok) throw new Error('Error al reordenar modulos')
}

import { api } from './client'

// ═══════════════════════════════════════
// Modules
// ═══════════════════════════════════════

export async function fetchModules(): Promise<import('../types').ModuleInfo[]> {
  const res = await api('/api/modules')
  if (!res.ok) throw new Error('Error al obtener modulos')
  return res.json()
}

/** Apagar un modulo que otro usa responde 409 con el motivo: se reintenta con `force` */
export class ModuleInUseError extends Error {}

/** Devuelve los modulos que este usa y estan apagados (al encender) */
export async function toggleModule(id: string, enabled: boolean, force = false): Promise<string[]> {
  const res = await api(`/api/modules/${id}`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ enabled, force }),
  })
  if (res.status === 409) throw new ModuleInUseError(await res.text())
  if (!res.ok) throw new Error('Error al cambiar estado del modulo')
  const data = await res.json().catch(() => ({}))
  return Array.isArray(data.missing) ? data.missing : []
}

export async function reorderModules(order: { id: string; order: number }[]): Promise<void> {
  const res = await api('/api/modules/order', {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(order),
  })
  if (!res.ok) throw new Error('Error al reordenar modulos')
}

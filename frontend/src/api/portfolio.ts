import { api } from './client'

// --- Portfolio ---

export async function fetchPortfolio(): Promise<import('../types').PortfolioEntry[]> {
  const res = await api('/api/portfolio')
  if (!res.ok) throw new Error('Error al obtener portafolio')
  return res.json()
}

export async function createPortfolioEntry(data: Partial<import('../types').PortfolioEntry>): Promise<import('../types').PortfolioEntry> {
  const res = await api('/api/portfolio', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(data),
  })
  if (!res.ok) throw new Error('Error al crear entrada')
  return res.json()
}

export async function updatePortfolioEntry(id: string, data: Partial<import('../types').PortfolioEntry>): Promise<import('../types').PortfolioEntry> {
  const res = await api(`/api/portfolio/${id}`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(data),
  })
  if (!res.ok) throw new Error('Error al actualizar entrada')
  return res.json()
}

export async function deletePortfolioEntry(id: string): Promise<void> {
  const res = await api(`/api/portfolio/${id}`, { method: 'DELETE' })
  if (!res.ok) throw new Error('Error al eliminar entrada')
}

export async function togglePortfolioRequirement(entryId: string, reqId: string): Promise<import('../types').PortfolioEntry> {
  const res = await api(`/api/portfolio/${entryId}/requirements/${reqId}/toggle`, { method: 'POST' })
  if (!res.ok) throw new Error('Error al cambiar requisito')
  return res.json()
}

export async function togglePortfolioMilestone(entryId: string, milId: string): Promise<import('../types').PortfolioEntry> {
  const res = await api(`/api/portfolio/${entryId}/milestones/${milId}/toggle`, { method: 'POST' })
  if (!res.ok) throw new Error('Error al cambiar hito')
  return res.json()
}

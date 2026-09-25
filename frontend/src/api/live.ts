import { api } from './client'

// --- Eventos en tiempo real ---

/** Ticket de un solo uso (30 s) para abrir un WebSocket sin poner el token en la URL */
export async function createWsTicket(): Promise<string> {
  const res = await api('/api/live/ticket', { method: 'POST' })
  if (!res.ok) throw new Error('No se pudo obtener ticket')
  return res.json()
}

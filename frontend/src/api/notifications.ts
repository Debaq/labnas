import { api } from './client'

// --- Notifications (Telegram) ---

export async function fetchNotificationConfig(): Promise<import('../types').NotificationConfig> {
  const res = await api('/api/notifications/telegram')
  if (!res.ok) throw new Error('Error al obtener config de notificaciones')
  return res.json()
}

export async function setBotToken(token: string): Promise<import('../types').NotificationConfig> {
  const res = await api('/api/notifications/telegram/token', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ token }),
  })
  if (!res.ok) {
    const text = await res.text()
    throw new Error(text || 'Error al configurar bot')
  }
  return res.json()
}

export async function deleteBotToken(): Promise<void> {
  const res = await api('/api/notifications/telegram/token', { method: 'DELETE' })
  if (!res.ok) throw new Error('Error al eliminar bot')
}

export async function setChatRole(chatId: number, role: string, permissions?: import('../types').UserPermissions): Promise<void> {
  const res = await api(`/api/notifications/telegram/chat/${chatId}/role`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ role, permissions }),
  })
  if (!res.ok) throw new Error('Error al cambiar rol')
}

export async function deleteTelegramChat(chatId: number): Promise<void> {
  const res = await api(`/api/notifications/telegram/chat/${chatId}`, { method: 'DELETE' })
  if (!res.ok) throw new Error('Error al eliminar chat')
}

export async function sendTestTelegram(): Promise<string> {
  const res = await api('/api/notifications/telegram/test', { method: 'POST' })
  return res.text()
}

export async function setNotificationSchedule(schedule: { daily_enabled: boolean; daily_hour: number; daily_minute: number }): Promise<void> {
  const res = await api('/api/notifications/schedule', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(schedule),
  })
  if (!res.ok) throw new Error('Error al configurar horario')
}

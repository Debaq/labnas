import { api, jsonOrThrow } from './client'

// --- Password ---

export async function changePassword(currentPassword: string, newPassword: string): Promise<void> {
  const res = await api('/api/auth/password', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ current_password: currentPassword, new_password: newPassword }),
  })
  if (!res.ok) {
    const text = await res.text()
    throw new Error(text || 'Error al cambiar contrasena')
  }
}

// --- Rename ---

export async function renameUser(newUsername: string): Promise<{ token: string; username: string; role: import('../types').UserRole; permissions: import('../types').UserPermissions }> {
  const res = await api('/api/auth/rename', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ new_username: newUsername }),
  })
  if (!res.ok) {
    const text = await res.text()
    throw new Error(text || 'Error al renombrar usuario')
  }
  return res.json()
}

// --- Linking ---

export async function generateLinkCode(token: string): Promise<string> {
  const res = await api('/api/auth/link-code', {
    method: 'POST',
    headers: { Authorization: `Bearer ${token}` },
  })
  if (!res.ok) throw new Error('Error al generar codigo')
  return res.text()
}

export async function adminLinkChat(chatId: number, webUsername: string): Promise<void> {
  const res = await api(`/api/notifications/telegram/chat/${chatId}/link`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ web_username: webUsername }),
  })
  if (!res.ok) throw new Error('Error al vincular')
}

// --- Web Users ---

export async function fetchUsernames(): Promise<string[]> {
  const res = await api('/api/auth/usernames')
  if (!res.ok) throw new Error('Error al obtener usernames')
  return res.json()
}

export async function fetchWebUsers(): Promise<{ username: string; role: import('../types').UserRole; permissions: import('../types').UserPermissions; totp_enabled?: boolean }[]> {
  const res = await api('/api/auth/users')
  if (!res.ok) throw new Error('Error al obtener usuarios')
  return res.json()
}

export async function setWebUserRole(username: string, role: string, permissions?: import('../types').UserPermissions): Promise<void> {
  const res = await api(`/api/auth/users/${encodeURIComponent(username)}/role`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ role, permissions }),
  })
  if (!res.ok) throw new Error('Error al cambiar rol')
}

export async function deleteWebUser(username: string): Promise<void> {
  const res = await api(`/api/auth/users/${encodeURIComponent(username)}`, { method: 'DELETE' })
  if (!res.ok) throw new Error('Error al eliminar usuario')
}

// --- Doble factor (TOTP) ---

export interface TwoFactorStatus {
  enabled: boolean
  app_password: boolean
  recovery_left: number
}

export interface TwoFactorSetup {
  secret: string
  uri: string
  qr_svg: string
}

function post(path: string, body: unknown) {
  return api(path, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) })
}

export async function fetchTwoFactor(): Promise<TwoFactorStatus> {
  return jsonOrThrow(await api('/api/auth/2fa'), 'Error al obtener doble factor')
}

export async function startTwoFactor(password: string): Promise<TwoFactorSetup> {
  return jsonOrThrow(await post('/api/auth/2fa/setup', { password }), 'Error al iniciar la activacion')
}

/** Devuelve los codigos de recuperacion (se muestran una sola vez) */
export async function enableTwoFactor(code: string): Promise<string[]> {
  const r = await jsonOrThrow<{ recovery_codes: string[] }>(await post('/api/auth/2fa/enable', { code }), 'Codigo incorrecto')
  return r.recovery_codes
}

export async function disableTwoFactor(password: string, code: string): Promise<void> {
  const res = await post('/api/auth/2fa/disable', { password, code })
  if (!res.ok) throw new Error((await res.text()) || 'Error al desactivar')
}

export async function regenerateRecoveryCodes(code: string): Promise<string[]> {
  const r = await jsonOrThrow<{ recovery_codes: string[] }>(await post('/api/auth/2fa/recovery', { code }), 'Error al regenerar')
  return r.recovery_codes
}

export async function createAppPassword(code: string): Promise<string> {
  const r = await jsonOrThrow<{ password: string }>(await post('/api/auth/2fa/app-password', { code }), 'Error al generar')
  return r.password
}

export async function resetUserTwoFactor(username: string): Promise<void> {
  const res = await api(`/api/auth/users/${encodeURIComponent(username)}/2fa`, { method: 'DELETE' })
  if (!res.ok) throw new Error((await res.text()) || 'Error al desactivar el doble factor')
}

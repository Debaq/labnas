import { api, jsonOrThrow } from './client'

/** Preferencias de la interfaz guardadas en la cuenta (las usa tambien labnas-tray) */
export interface Preferences {
  theme: string | null
}

export async function fetchPreferences(): Promise<Preferences> {
  return jsonOrThrow(await api('/api/auth/preferences'), 'Error cargando preferencias')
}

export async function savePreferences(prefs: Preferences): Promise<Preferences> {
  const res = await api('/api/auth/preferences', {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(prefs),
  })
  return jsonOrThrow(res, 'Error guardando preferencias')
}

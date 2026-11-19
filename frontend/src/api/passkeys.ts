import { api } from './client'

/** ¿El navegador puede usar passkeys aqui? (HTTPS y nombre de dominio, no IP) */
export function passkeysSupported(): boolean {
  if (typeof window === 'undefined' || !window.PublicKeyCredential || !window.isSecureContext) return false
  const host = window.location.hostname
  return !/^\d+\.\d+\.\d+\.\d+$/.test(host) && !host.includes(':')
}

function toB64url(buf: ArrayBuffer): string {
  const bytes = new Uint8Array(buf)
  let s = ''
  for (const b of bytes) s += String.fromCharCode(b)
  return btoa(s).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '')
}

function fromB64url(s: string): ArrayBuffer {
  const b64 = s.replace(/-/g, '+').replace(/_/g, '/') + '==='.slice((s.length + 3) % 4)
  const bin = atob(b64)
  const out = new Uint8Array(bin.length)
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i)
  return out.buffer
}

async function jsonOrError<T>(res: Response, fallback: string): Promise<T> {
  if (!res.ok) throw new Error((await res.text()) || fallback)
  return res.json()
}

interface CredDescriptor { type: 'public-key'; id: string }

export interface PasskeyInfo {
  id: string
  name: string
  rp_id: string
  created_at: string
  last_used: string | null
}

export async function fetchPasskeys(): Promise<PasskeyInfo[]> {
  return jsonOrError(await api('/api/auth/passkeys'), 'Error al obtener passkeys')
}

export async function deletePasskey(id: string): Promise<void> {
  const res = await api(`/api/auth/passkeys/${encodeURIComponent(id)}`, { method: 'DELETE' })
  if (!res.ok) throw new Error((await res.text()) || 'Error al eliminar')
}

/** Crea una passkey para el usuario con sesion (huella, Face ID, Windows Hello o llave) */
export async function registerPasskey(name: string): Promise<void> {
  const opts = await jsonOrError<{
    challenge: string
    rp: { id: string; name: string }
    user: { id: string; name: string; displayName: string }
    pubKeyCredParams: { type: 'public-key'; alg: number }[]
    timeout: number
    attestation: AttestationConveyancePreference
    authenticatorSelection: AuthenticatorSelectionCriteria
    excludeCredentials: CredDescriptor[]
  }>(await api('/api/auth/passkey/register/options', { method: 'POST' }), 'No se pudo iniciar el registro')

  const cred = await navigator.credentials.create({
    publicKey: {
      ...opts,
      challenge: fromB64url(opts.challenge),
      user: { ...opts.user, id: fromB64url(opts.user.id) },
      excludeCredentials: opts.excludeCredentials.map(c => ({ type: c.type, id: fromB64url(c.id) })),
    },
  }) as PublicKeyCredential | null
  if (!cred) throw new Error('Registro cancelado')
  const resp = cred.response as AuthenticatorAttestationResponse
  const res = await api('/api/auth/passkey/register', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      name,
      id: toB64url(cred.rawId),
      clientDataJSON: toB64url(resp.clientDataJSON),
      attestationObject: toB64url(resp.attestationObject),
    }),
  })
  await jsonOrError(res, 'No se pudo guardar la passkey')
}

/** Inicia sesion con una passkey; devuelve la misma respuesta que el login con contraseña */
export async function loginWithPasskey(username?: string): Promise<{ username: string; role: string; permissions: unknown; enabled_modules: unknown }> {
  const opts = await jsonOrError<{ challenge: string; rpId: string; timeout: number; userVerification: UserVerificationRequirement; allowCredentials: CredDescriptor[] }>(
    await api('/api/auth/passkey/login/options', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ username: username || null }),
    }),
    'No se pudo iniciar',
  )
  const cred = await navigator.credentials.get({
    publicKey: {
      ...opts,
      challenge: fromB64url(opts.challenge),
      allowCredentials: opts.allowCredentials.map(c => ({ type: c.type, id: fromB64url(c.id) })),
    },
  }) as PublicKeyCredential | null
  if (!cred) throw new Error('Inicio cancelado')
  const resp = cred.response as AuthenticatorAssertionResponse
  const res = await api('/api/auth/passkey/login', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      id: toB64url(cred.rawId),
      clientDataJSON: toB64url(resp.clientDataJSON),
      authenticatorData: toB64url(resp.authenticatorData),
      signature: toB64url(resp.signature),
    }),
  })
  return jsonOrError(res, 'No se pudo iniciar sesion con la passkey')
}

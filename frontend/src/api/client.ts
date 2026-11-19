/**
 * fetch de la app. La sesion viaja en una cookie HttpOnly (el navegador la manda
 * sola; JavaScript no puede leerla, asi que un XSS no puede robar el token).
 * Toda peticion lleva X-LabNAS-CSRF: con sesion por cookie el servidor la exige
 * en los metodos que modifican.
 */
export const UNAUTHORIZED_EVENT = 'labnas:unauthorized'

export function authHeaders(extra?: Record<string, string>): Record<string, string> {
  return { ...extra, 'X-LabNAS-CSRF': '1' }
}

export async function api(url: string, opts?: RequestInit): Promise<Response> {
  const headers = authHeaders(
    opts?.headers ? Object.fromEntries(
      opts.headers instanceof Headers
        ? opts.headers.entries()
        : Object.entries(opts.headers as Record<string, string>)
    ) : undefined
  )

  // FormData: el navegador pone el Content-Type con el boundary
  const isFormData = opts?.body instanceof FormData
  if (isFormData) delete headers['Content-Type']

  const res = await fetch(url, { ...opts, credentials: 'same-origin', headers })
  // Sesion vencida o revocada: la app vuelve al login (una sola via, ver AuthContext)
  if (res.status === 401 && !url.startsWith('/api/auth/login') && !url.startsWith('/api/auth/register')) {
    window.dispatchEvent(new Event(UNAUTHORIZED_EVENT))
  }
  return res
}

/** JSON de la respuesta o Error con el texto del servidor */
export async function jsonOrThrow<T>(res: Response, fallback: string): Promise<T> {
  if (!res.ok) throw new Error((await res.text()) || fallback)
  return res.json()
}

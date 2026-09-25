/** fetch con el token de sesion (lo usan todos los modulos de ../api) */
// Auth-aware fetch wrapper
export function authHeaders(extra?: Record<string, string>): Record<string, string> {
  const headers: Record<string, string> = { ...extra }
  try {
    const saved = localStorage.getItem('labnas_auth')
    if (saved) {
      const { token } = JSON.parse(saved)
      if (token) headers['Authorization'] = `Bearer ${token}`
    }
  } catch {}
  return headers
}

export async function api(url: string, opts?: RequestInit): Promise<Response> {
  const headers = authHeaders(
    opts?.headers ? Object.fromEntries(
      opts.headers instanceof Headers
        ? opts.headers.entries()
        : Object.entries(opts.headers as Record<string, string>)
    ) : undefined
  )

  // Don't set auth header for FormData (browser sets content-type with boundary)
  const isFormData = opts?.body instanceof FormData

  return fetch(url, {
    ...opts,
    headers: isFormData
      ? (headers['Authorization'] ? { Authorization: headers['Authorization'] } : {})
      : headers,
  })
}

/** JSON de la respuesta o Error con el texto del servidor */
export async function jsonOrThrow<T>(res: Response, fallback: string): Promise<T> {
  if (!res.ok) throw new Error((await res.text()) || fallback)
  return res.json()
}

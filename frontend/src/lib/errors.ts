/** Mensaje legible de cualquier error capturado (catch recibe `unknown`) */
export function errorMessage(e: unknown): string {
  return e instanceof Error ? e.message : String(e)
}

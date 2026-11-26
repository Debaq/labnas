import { useCallback, useEffect, useState } from 'react'
import { fetchLevels, type RecordKind, type Visibility } from '../../api'

/**
 * Nivel de acceso de los registros de una lista (los que no aparecen son privados).
 * `set` actualiza uno tras cambiarlo en la ventana de acceso.
 */
export function useAccessLevels(kind: RecordKind, ids: string[]) {
  const [levels, setLevels] = useState<Record<string, Visibility>>({})
  const key = ids.join('\n')
  useEffect(() => {
    let alive = true
    fetchLevels(kind, key ? key.split('\n') : []).then(l => { if (alive) setLevels(l) }).catch(() => {})
    return () => { alive = false }
  }, [kind, key])
  const set = useCallback((id: string, v: Visibility) => setLevels(l => ({ ...l, [id]: v })), [])
  return { levels, level: (id: string): Visibility => levels[id] ?? 'privado', set }
}

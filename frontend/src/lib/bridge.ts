import { useEffect, useState } from 'react'
import { fetchHealth } from '../api/system'

// Se consulta una vez por carga de pagina: el origen no cambia sin recargar
let viaBridge: Promise<boolean> | null = null

/** true si esta sesion entra por el puente PHP (internet): sin terminal ni admin */
export function useViaBridge(): boolean {
  const [via, setVia] = useState(false)
  useEffect(() => {
    if (!viaBridge) viaBridge = fetchHealth().then(h => h.via === 'bridge').catch(() => false)
    let alive = true
    viaBridge.then(v => { if (alive) setVia(v) })
    return () => { alive = false }
  }, [])
  return via
}

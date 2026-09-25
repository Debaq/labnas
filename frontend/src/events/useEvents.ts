import { createContext, useContext, useEffect, useRef } from 'react'

/** Evento del bus del servidor (WebSocket /api/live) */
export interface ServerEvent<T = unknown> {
  kind: string
  data: T
}

export type EventHandler<T = unknown> = (data: T) => void

export interface EventsContextValue {
  /** Conectado al bus: los componentes pueden dejar de consultar periodicamente */
  connected: boolean
  subscribe: (kind: string, handler: EventHandler) => () => void
}

export const EventsContext = createContext<EventsContextValue>({
  connected: false,
  subscribe: () => () => {},
})

/** true si hay conexion en vivo con el servidor */
export function useEventsConnected(): boolean {
  return useContext(EventsContext).connected
}

/**
 * Ejecuta `handler` con cada evento `kind`. Tambien se dispara con kind "resync"
 * (reconexion o eventos perdidos) si se pasa `onResync`.
 */
export function useEvent<T = unknown>(kind: string, handler: EventHandler<T>, onResync?: () => void): void {
  const { subscribe } = useContext(EventsContext)
  const handlerRef = useRef(handler)
  const resyncRef = useRef(onResync)

  useEffect(() => {
    handlerRef.current = handler
    resyncRef.current = onResync
  })

  useEffect(() => {
    const off = subscribe(kind, (d) => handlerRef.current(d as T))
    const offResync = subscribe('resync', () => resyncRef.current?.())
    return () => { off(); offResync() }
  }, [kind, subscribe])
}

import { useCallback, useEffect, useRef, useState, type ReactNode } from 'react'
import { useAuth } from '../auth/AuthContext'
import { useToast } from '../components/ToastContext'
import { createWsTicket } from '../api'
import { EventsContext, type EventHandler, type ServerEvent } from './useEvents'

interface NotifyData {
  level: 'info' | 'success' | 'warning' | 'error'
  title: string
  body: string
}

/** El visor de escritorio (wry) inyecta window.ipc: ahi van las notificaciones nativas */
interface WryWindow extends Window {
  ipc?: { postMessage: (msg: string) => void }
}

const MAX_BACKOFF_MS = 30_000
/** Eventos que el servidor manda siempre o que genera el propio cliente */
const LOCAL_KINDS = new Set(['resync', 'notify', 'auth.changed'])

/**
 * Una conexion WebSocket por pestaña al bus del servidor, con reconexion.
 * Muestra "notify" como toast (y nativa en el visor) y recarga la sesion con "auth.changed".
 */
export default function EventsProvider({ children }: { children: ReactNode }) {
  const { user, refreshUser } = useAuth()
  const { addToast } = useToast()
  const [connected, setConnected] = useState(false)
  const listeners = useRef(new Map<string, Set<EventHandler>>())
  const wsRef = useRef<WebSocket | null>(null)

  /** Avisa al servidor que temas escucha esta pestaña (para no hacer trabajo de mas) */
  const sendInterest = useCallback((op: 'sub' | 'unsub', kind: string) => {
    if (LOCAL_KINDS.has(kind)) return
    const ws = wsRef.current
    if (ws?.readyState === WebSocket.OPEN) ws.send(JSON.stringify({ [op]: kind }))
  }, [])

  const dispatch = useCallback((kind: string, data: unknown) => {
    listeners.current.get(kind)?.forEach((h) => {
      try { h(data) } catch (e) { console.error(`[eventos] ${kind}:`, e) }
    })
  }, [])

  const subscribe = useCallback((kind: string, handler: EventHandler) => {
    const set = listeners.current.get(kind) ?? new Set()
    if (set.size === 0) sendInterest('sub', kind)
    set.add(handler)
    listeners.current.set(kind, set)
    return () => {
      set.delete(handler)
      if (set.size === 0) sendInterest('unsub', kind)
    }
  }, [sendInterest])

  // Callbacks estables para no reconectar cuando cambian
  const refreshRef = useRef(refreshUser)
  const toastRef = useRef(addToast)
  useEffect(() => {
    refreshRef.current = refreshUser
    toastRef.current = addToast
  })

  const token = user?.token
  useEffect(() => {
    if (!token) return
    let ws: WebSocket | null = null
    let retry: ReturnType<typeof setTimeout> | null = null
    let backoff = 1000
    let stopped = false
    let wasConnected = false

    const onEvent = (ev: ServerEvent) => {
      if (ev.kind === 'notify') {
        const n = ev.data as NotifyData
        const type = n.level === 'success' ? 'success' : n.level === 'error' ? 'error' : 'info'
        toastRef.current(n.body ? `${n.title}: ${n.body}` : n.title, type)
        ;(window as WryWindow).ipc?.postMessage(JSON.stringify({ type: 'notify', title: n.title, body: n.body }))
      } else if (ev.kind === 'auth.changed') {
        refreshRef.current()
      }
      dispatch(ev.kind, ev.data)
    }

    const connect = async () => {
      try {
        const ticket = await createWsTicket()
        if (stopped) return
        const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:'
        ws = new WebSocket(`${protocol}//${window.location.host}/api/live?ticket=${encodeURIComponent(ticket)}`)
        wsRef.current = ws
        ws.onopen = () => {
          backoff = 1000
          setConnected(true)
          // Declarar los temas que ya tienen oyentes
          listeners.current.forEach((set, kind) => { if (set.size > 0) sendInterest('sub', kind) })
          // Tras reconectar pudimos perder eventos: que cada vista recargue
          if (wasConnected) dispatch('resync', null)
          wasConnected = true
        }
        ws.onmessage = (m) => {
          try { onEvent(JSON.parse(m.data) as ServerEvent) } catch { /* mensaje invalido */ }
        }
        ws.onclose = () => {
          setConnected(false)
          ws = null
          wsRef.current = null
          scheduleRetry()
        }
      } catch {
        scheduleRetry()
      }
    }

    const scheduleRetry = () => {
      if (stopped) return
      retry = setTimeout(connect, backoff)
      backoff = Math.min(backoff * 2, MAX_BACKOFF_MS)
    }

    connect()
    return () => {
      stopped = true
      if (retry) clearTimeout(retry)
      if (ws) { ws.onclose = null; ws.close() }
      wsRef.current = null
      setConnected(false)
    }
  }, [token, dispatch, sendInterest])

  return (
    <EventsContext.Provider value={{ connected, subscribe }}>
      {children}
    </EventsContext.Provider>
  )
}

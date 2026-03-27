import { useEffect, useRef, useState } from 'react'
import { useLocation } from 'react-router-dom'
import { Terminal } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import { WebLinksAddon } from '@xterm/addon-web-links'
import '@xterm/xterm/css/xterm.css'

export default function PersistentTerminal() {
  const location = useLocation()
  const isVisible = location.pathname === '/terminal'
  const containerRef = useRef<HTMLDivElement>(null)
  const termRef = useRef<Terminal | null>(null)
  const wsRef = useRef<WebSocket | null>(null)
  const fitRef = useRef<FitAddon | null>(null)
  const [initialized, setInitialized] = useState(false)
  const pendingCmd = isVisible ? (location.state as any)?.commands as string | undefined : undefined

  // Inicializar solo la primera vez que se visita /terminal
  useEffect(() => {
    if (!isVisible || initialized || !containerRef.current) return

    const container = containerRef.current
    const cs = getComputedStyle(document.documentElement)
    const bgColor = cs.getPropertyValue('--bg-primary').trim() || '#282a36'

    const term = new Terminal({
      cursorBlink: true,
      cursorStyle: 'bar',
      fontSize: 14,
      fontFamily: "'JetBrains Mono', 'Fira Code', 'Cascadia Code', 'Courier New', monospace",
      lineHeight: 1.2,
      scrollback: 10000,
      convertEol: false,
      theme: {
        background: bgColor,
        foreground: cs.getPropertyValue('--text-primary').trim() || '#f8f8f2',
        cursor: cs.getPropertyValue('--accent').trim() || '#bd93f9',
        cursorAccent: bgColor,
        selectionBackground: 'rgba(189, 147, 249, 0.3)',
        selectionForeground: '#ffffff',
        black: '#21222c',
        red: '#ff5555',
        green: '#50fa7b',
        yellow: '#f1fa8c',
        blue: '#bd93f9',
        magenta: '#ff79c6',
        cyan: '#8be9fd',
        white: '#f8f8f2',
        brightBlack: '#6272a4',
        brightRed: '#ff6e6e',
        brightGreen: '#69ff94',
        brightYellow: '#ffffa5',
        brightBlue: '#d6acff',
        brightMagenta: '#ff92df',
        brightCyan: '#a4ffff',
        brightWhite: '#ffffff',
      },
      allowProposedApi: true,
    })

    const fitAddon = new FitAddon()
    term.loadAddon(fitAddon)
    term.loadAddon(new WebLinksAddon())
    term.open(container)

    termRef.current = term
    fitRef.current = fitAddon

    setTimeout(() => fitAddon.fit(), 50)

    const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:'
    let wsUrl = `${protocol}//${window.location.host}/api/terminal`
    try {
      const saved = localStorage.getItem('labnas_auth')
      if (saved) {
        const { token } = JSON.parse(saved)
        if (token) wsUrl += `?token=${encodeURIComponent(token)}`
      }
    } catch {}
    const ws = new WebSocket(wsUrl)
    ws.binaryType = 'arraybuffer'
    wsRef.current = ws

    ws.onopen = () => {
      setTimeout(() => {
        fitAddon.fit()
        ws.send('\x01' + JSON.stringify({ cols: term.cols, rows: term.rows }))
        if (pendingCmd) {
          setTimeout(() => ws.send(pendingCmd + '\n'), 300)
          window.history.replaceState({}, document.title)
        }
      }, 100)
    }

    ws.onmessage = (event) => {
      if (event.data instanceof ArrayBuffer) {
        term.write(new Uint8Array(event.data))
      } else {
        term.write(event.data)
      }
    }

    ws.onerror = () => {
      term.write('\r\n\x1b[1;31m[Error de conexion]\x1b[0m\r\n')
    }

    ws.onclose = () => {
      term.write('\r\n\x1b[1;31m[Conexion cerrada]\x1b[0m\r\n')
      // Permitir reinicializar si se cierra la conexion
      setInitialized(false)
      termRef.current = null
      wsRef.current = null
      fitRef.current = null
    }

    term.onData((data) => {
      if (ws.readyState === WebSocket.OPEN) ws.send(data)
    })

    term.onBinary((data) => {
      if (ws.readyState === WebSocket.OPEN) {
        const buf = new Uint8Array(data.length)
        for (let i = 0; i < data.length; i++) buf[i] = data.charCodeAt(i) & 255
        ws.send(buf)
      }
    })

    term.onResize(({ cols, rows }) => {
      if (ws.readyState === WebSocket.OPEN) {
        ws.send('\x01' + JSON.stringify({ cols, rows }))
      }
    })

    setInitialized(true)
  }, [isVisible, initialized])

  // Fit + focus cuando se vuelve visible
  useEffect(() => {
    if (isVisible && fitRef.current && termRef.current) {
      setTimeout(() => {
        fitRef.current?.fit()
        termRef.current?.focus()
      }, 50)
    }
  }, [isVisible])

  // Resize handler global
  useEffect(() => {
    if (!initialized) return
    const onResize = () => { if (isVisible) fitRef.current?.fit() }
    window.addEventListener('resize', onResize)
    return () => window.removeEventListener('resize', onResize)
  }, [initialized, isVisible])

  return (
    <div
      style={{
        display: isVisible ? 'flex' : 'none',
        flexDirection: 'column',
        position: 'absolute',
        inset: 0,
        padding: '2rem',
        backgroundColor: 'var(--bg-primary)',
      }}
    >
      <div className="flex items-center gap-3 mb-3">
        <h2 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>
          Terminal
        </h2>
        <span
          className="px-2.5 py-0.5 rounded-full text-xs font-medium"
          style={{ backgroundColor: 'var(--success-alpha)', color: 'var(--success)' }}
        >
          {initialized ? 'bash' : 'desconectada'}
        </span>
      </div>
      <div
        ref={containerRef}
        className="flex-1 rounded-xl overflow-hidden p-1"
        style={{ backgroundColor: 'var(--bg-secondary)', border: '1px solid var(--border)' }}
      />
    </div>
  )
}

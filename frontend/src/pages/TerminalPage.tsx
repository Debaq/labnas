import { useEffect, useRef } from 'react'
import { useLocation } from 'react-router-dom'
import { Terminal } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import { WebLinksAddon } from '@xterm/addon-web-links'
import '@xterm/xterm/css/xterm.css'

export default function TerminalPage() {
  const containerRef = useRef<HTMLDivElement>(null)
  const cleanupRef = useRef<(() => void) | null>(null)
  const location = useLocation()
  const pendingCmd = (location.state as any)?.commands as string | undefined

  useEffect(() => {
    // Cleanup previous instance if any (StrictMode)
    if (cleanupRef.current) {
      cleanupRef.current()
      cleanupRef.current = null
    }

    const container = containerRef.current
    if (!container) return

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

    // Fit after render
    setTimeout(() => fitAddon.fit(), 50)

    // WebSocket (con token via query param, ya que WS no soporta headers custom)
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

    ws.onopen = () => {
      console.log('[LabNAS] Terminal WebSocket connected')
      setTimeout(() => {
        fitAddon.fit()
        ws.send('\x01' + JSON.stringify({ cols: term.cols, rows: term.rows }))
        // Auto-type commands if navigated with state (e.g., from autostart setup)
        if (pendingCmd) {
          setTimeout(() => {
            ws.send(pendingCmd + '\n')
          }, 600)
          window.history.replaceState({}, document.title)
        }
      }, 150)
    }

    ws.onmessage = (event) => {
      if (event.data instanceof ArrayBuffer) {
        term.write(new Uint8Array(event.data))
      } else {
        term.write(event.data)
      }
    }

    ws.onerror = (e) => {
      console.error('[LabNAS] Terminal WebSocket error:', e)
      term.write('\r\n\x1b[1;31m[Error de conexion - verifica que el backend este corriendo]\x1b[0m\r\n')
    }

    ws.onclose = (e) => {
      console.log('[LabNAS] Terminal WebSocket closed:', e.code, e.reason)
      term.write('\r\n\x1b[1;31m[Conexion cerrada]\x1b[0m\r\n')
    }

    // Input -> WebSocket
    term.onData((data) => {
      if (ws.readyState === WebSocket.OPEN) {
        ws.send(data)
      }
    })

    term.onBinary((data) => {
      if (ws.readyState === WebSocket.OPEN) {
        const buf = new Uint8Array(data.length)
        for (let i = 0; i < data.length; i++) buf[i] = data.charCodeAt(i) & 255
        ws.send(buf)
      }
    })

    // Resize
    term.onResize(({ cols, rows }) => {
      if (ws.readyState === WebSocket.OPEN) {
        ws.send('\x01' + JSON.stringify({ cols, rows }))
      }
    })

    const onWindowResize = () => fitAddon.fit()
    window.addEventListener('resize', onWindowResize)

    // Focus
    term.focus()
    container.addEventListener('click', () => term.focus())

    // Store cleanup
    const cleanup = () => {
      window.removeEventListener('resize', onWindowResize)
      ws.close()
      term.dispose()
    }
    cleanupRef.current = cleanup

    return cleanup
  }, [])

  return (
    <div className="flex flex-col" style={{ height: 'calc(100vh - 140px)' }}>
      <div className="flex items-center justify-between mb-3">
        <div className="flex items-center gap-3">
          <h2 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>
            Terminal
          </h2>
          <span
            className="px-2.5 py-0.5 rounded-full text-xs font-medium"
            style={{ backgroundColor: 'var(--success-alpha)', color: 'var(--success)' }}
          >
            bash
          </span>
        </div>
      </div>
      <div
        ref={containerRef}
        className="flex-1 rounded-xl overflow-hidden p-1"
        style={{ backgroundColor: 'var(--bg-secondary)', border: '1px solid var(--border)' }}
      />
    </div>
  )
}

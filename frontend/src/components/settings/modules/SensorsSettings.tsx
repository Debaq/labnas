import { useEffect, useState } from 'react'
import { Usb } from 'lucide-react'
import { configureReceiver, fetchReceiverStatus } from '../../../api'
import type { ReceiverStatus } from '../../../types'
import { errorMessage } from '../../../lib/errors'

/** Puerto serie del receptor ESP-NOW */
export default function SensorsSettings() {
  const [receiver, setReceiver] = useState<ReceiverStatus | null>(null)
  const [port, setPort] = useState('')
  const [busy, setBusy] = useState(false)
  const [msg, setMsg] = useState<{ ok: boolean; text: string } | null>(null)

  useEffect(() => {
    fetchReceiverStatus().then(r => { setReceiver(r); setPort(r.port ?? '') }).catch(() => {})
  }, [])

  async function save() {
    if (!port.trim()) return
    setBusy(true)
    setMsg(null)
    try {
      await configureReceiver(port.trim())
      setReceiver(await fetchReceiverStatus())
      setMsg({ ok: true, text: 'Puerto guardado' })
    } catch (e) {
      setMsg({ ok: false, text: errorMessage(e) })
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="rounded-xl p-5 space-y-3" style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}>
      <div className="flex items-center gap-2 text-sm" style={{ color: receiver?.connected ? 'var(--success)' : 'var(--text-secondary)' }}>
        <Usb size={16} />
        {receiver?.connected ? `Receptor conectado en ${receiver.port}` : receiver?.port ? `Receptor desconectado (${receiver.port})` : 'Sin receptor configurado'}
        {receiver?.error && !receiver.connected && <span className="text-xs" style={{ color: 'var(--danger)' }}>{receiver.error}</span>}
      </div>
      <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>
        Puerto serie del receptor ESP-NOW (los sensores WiFi no lo necesitan).
      </p>
      <div className="flex items-center gap-2">
        <input
          value={port}
          onChange={e => setPort(e.target.value)}
          placeholder="/dev/ttyUSB0"
          className="flex-1 px-3 py-2 rounded-lg text-sm font-mono"
          style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
        />
        <button disabled={busy} onClick={save} className="px-4 py-2 rounded-lg text-sm" style={{ backgroundColor: 'var(--accent)', color: '#fff' }}>
          Guardar
        </button>
      </div>
      {msg && <p className="text-xs" style={{ color: msg.ok ? 'var(--success)' : 'var(--danger)' }}>{msg.text}</p>}
    </div>
  )
}

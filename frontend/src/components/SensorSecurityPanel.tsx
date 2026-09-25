import { useCallback, useEffect, useState } from 'react'
import { KeyRound, ShieldCheck, Copy, Check, Loader2, X } from 'lucide-react'
import {
  fetchSensorDevices, createSensorToken, deleteSensorToken, fetchSensorSecurity, setSensorSecurity,
} from '../api'
import type { SensorDevice } from '../types'
import { errorMessage } from '../lib/errors'

const card = { backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }

/** Admin: tokens por dispositivo y "exigir token" para /api/sensors/data */
export default function SensorSecurityPanel() {
  const [devices, setDevices] = useState<SensorDevice[]>([])
  const [requireToken, setRequireToken] = useState(false)
  const [newToken, setNewToken] = useState<{ device: string; token: string } | null>(null)
  const [copied, setCopied] = useState(false)
  const [busy, setBusy] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)

  const load = useCallback(() => {
    Promise.all([fetchSensorDevices(), fetchSensorSecurity()])
      .then(([d, sec]) => { setDevices(d); setRequireToken(sec.require_token) })
      .catch((e) => setError(errorMessage(e)))
  }, [])

  useEffect(() => { load() }, [load])

  async function run(key: string, fn: () => Promise<void>) {
    setBusy(key)
    setError(null)
    try { await fn(); load() } catch (e) { setError(errorMessage(e)) } finally { setBusy(null) }
  }

  const withoutToken = devices.filter((d) => !d.has_token && d.status === 'accepted').length

  return (
    <div className="rounded-xl p-5 space-y-4" style={card}>
      <div className="flex items-center justify-between gap-3 flex-wrap">
        <div className="flex items-center gap-2">
          <ShieldCheck size={18} style={{ color: 'var(--accent)' }} />
          <span className="font-semibold text-sm" style={{ color: 'var(--text-primary)' }}>Seguridad de sensores</span>
        </div>
        <label className="flex items-center gap-2 text-sm" style={{ color: 'var(--text-primary)' }}>
          <input
            type="checkbox"
            checked={requireToken}
            disabled={busy === 'require'}
            onChange={(e) => {
              const v = e.target.checked
              if (v && withoutToken > 0 && !confirm(`${withoutToken} dispositivo(s) aceptado(s) no tienen token y dejaran de enviar datos. ¿Continuar?`)) return
              run('require', () => setSensorSecurity(v).then(() => undefined))
            }}
          />
          Exigir token a todos los sensores
        </label>
      </div>
      <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>
        Un sensor con token debe enviarlo siempre (header <code>X-Sensor-Token</code> o campo <code>token</code> del JSON).
        Sin "exigir token", los sensores sin token siguen funcionando; con la opcion activa se rechazan, igual que los dispositivos desconocidos.
      </p>

      {error && <p className="text-sm" style={{ color: 'var(--danger)' }}>{error}</p>}

      {newToken && (
        <div className="rounded-lg p-3 space-y-2" style={{ backgroundColor: 'var(--bg-tertiary)', border: '1px solid var(--warning)' }}>
          <div className="flex items-center justify-between">
            <span className="text-xs font-semibold" style={{ color: 'var(--warning)' }}>
              Token de {newToken.device}: copialo ahora, no se vuelve a mostrar
            </span>
            <button onClick={() => { setNewToken(null); setCopied(false) }} style={{ color: 'var(--text-secondary)' }}><X size={14} /></button>
          </div>
          <div className="flex items-center gap-2">
            <code className="flex-1 text-xs break-all font-mono" style={{ color: 'var(--text-primary)' }}>{newToken.token}</code>
            <button
              title="Copiar"
              onClick={() => navigator.clipboard.writeText(newToken.token).then(() => setCopied(true)).catch(() => {})}
              style={{ color: 'var(--accent)' }}
            >
              {copied ? <Check size={16} /> : <Copy size={16} />}
            </button>
          </div>
        </div>
      )}

      <div className="space-y-2">
        {devices.length === 0 && <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>Sin dispositivos.</p>}
        {devices.map((d) => {
          const name = d.name || d.mac
          return (
            <div key={d.id} className="flex items-center justify-between gap-3 px-3 py-2 rounded-lg" style={{ backgroundColor: 'var(--bg-tertiary)' }}>
              <div className="min-w-0">
                <p className="text-sm truncate" style={{ color: 'var(--text-primary)' }}>{name}</p>
                <p className="text-xs font-mono" style={{ color: 'var(--text-secondary)' }}>
                  {d.mac} · {d.has_token ? 'con token' : 'sin token'}
                </p>
              </div>
              <div className="flex items-center gap-2 shrink-0">
                {busy === d.id ? <Loader2 size={15} className="animate-spin" /> : (
                  <>
                    <button
                      onClick={() => {
                        if (d.has_token && !confirm(`Reemplazar el token de ${name}? El sensor dejara de enviar datos hasta actualizarlo.`)) return
                        run(d.id, async () => { const t = await createSensorToken(d.id); setNewToken({ device: name, token: t }); setCopied(false) })
                      }}
                      className="flex items-center gap-1 px-2 py-1 rounded-lg text-xs"
                      style={{ color: 'var(--accent)', border: '1px solid var(--border)' }}
                    >
                      <KeyRound size={13} /> {d.has_token ? 'Regenerar' : 'Generar token'}
                    </button>
                    {d.has_token && (
                      <button
                        onClick={() => { if (confirm(`Quitar el token de ${name}?`)) run(d.id, () => deleteSensorToken(d.id)) }}
                        className="px-2 py-1 rounded-lg text-xs"
                        style={{ color: 'var(--danger)', border: '1px solid var(--border)' }}
                      >
                        Quitar
                      </button>
                    )}
                  </>
                )}
              </div>
            </div>
          )
        })}
      </div>
    </div>
  )
}

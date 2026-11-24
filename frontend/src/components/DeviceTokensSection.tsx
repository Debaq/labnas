import { useEffect, useState } from 'react'
import { MonitorSmartphone, Trash2 } from 'lucide-react'
import { fetchDeviceTokens, deleteDeviceToken, type DeviceToken } from '../api'
import { errorMessage } from '../lib/errors'

/** Dispositivos vinculados a la cuenta (labnas-tray en la bandeja del sistema) */
export default function DeviceTokensSection() {
  const [list, setList] = useState<DeviceToken[]>([])
  const [error, setError] = useState<string | null>(null)

  async function load() {
    try { setList(await fetchDeviceTokens()) } catch { /* sin sesion */ }
  }
  useEffect(() => { void load() }, [])

  async function remove(d: DeviceToken) {
    if (!confirm(`Desvincular "${d.name}"? Su control de música dejará de funcionar.`)) return
    try { await deleteDeviceToken(d.id); await load() } catch (e) { setError(errorMessage(e)) }
  }

  return (
    <section className="mt-8">
      <div className="flex items-center gap-3 mb-4">
        <MonitorSmartphone size={22} style={{ color: 'var(--accent)' }} />
        <h2 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>Dispositivos vinculados</h2>
      </div>
      <div className="rounded-xl p-6 space-y-4" style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}>
        <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>
          Computadores con <strong>labnas-tray</strong>, el control de música en la bandeja del sistema (Windows y Linux).
          Se vinculan desde el propio icono, con «Vincular con mi cuenta», y solo pueden controlar el reproductor.
        </p>
        {list.length === 0 ? (
          <p className="text-xs" style={{ color: 'var(--text-secondary)', opacity: 0.7 }}>Ningún dispositivo vinculado.</p>
        ) : (
          <div className="space-y-2">
            {list.map(d => (
              <div key={d.id} className="flex items-center justify-between gap-3 text-sm">
                <div className="min-w-0">
                  <p className="truncate" style={{ color: 'var(--text-primary)' }}>{d.name}</p>
                  <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>
                    vinculado {new Date(d.created_at).toLocaleDateString()}
                    {d.last_used ? ` · último uso ${new Date(d.last_used).toLocaleString()}` : ' · sin uso'}
                  </p>
                </div>
                <button title="Desvincular" onClick={() => remove(d)} className="p-2 rounded-lg" style={{ color: 'var(--danger)' }}>
                  <Trash2 size={15} />
                </button>
              </div>
            ))}
          </div>
        )}
        {error && <p className="text-xs" style={{ color: 'var(--danger)' }}>{error}</p>}
      </div>
    </section>
  )
}

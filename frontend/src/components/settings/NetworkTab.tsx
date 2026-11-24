import { useEffect, useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { Globe, TerminalSquare } from 'lucide-react'
import { getMdnsStatus, setMdns } from '../../api'

export default function NetworkTab() {
  const navigate = useNavigate()
  const [mdns, setMdnsState] = useState<{ enabled: boolean; hostname: string; url: string } | null>(null)
  const [mdnsHostname, setMdnsHostname] = useState('')

  useEffect(() => {
    getMdnsStatus().then(s => { setMdnsState(s); setMdnsHostname(s.hostname) }).catch(() => {})
  }, [])

  return (
    <>
      {/* Tailscale - Remote Access (admin only) */}
      <section>
        <div className="flex items-center gap-3 mb-4">
          <Globe size={22} style={{ color: 'var(--accent)' }} />
          <h2 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>
            Acceso Remoto (Tailscale)
          </h2>
        </div>
        <div
          className="rounded-xl p-6"
          style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}
        >
          <p className="text-xs mb-4" style={{ color: 'var(--text-secondary)' }}>
            Tailscale permite acceder a LabNAS desde cualquier parte del mundo de forma segura, sin exponer puertos a internet.
            Funciona en Linux, Windows, macOS, iPhone y Android.
          </p>
          <div className="flex items-center gap-3">
            <button
              onClick={() => navigate('/terminal', { state: { commands: 'curl -fsSL https://tailscale.com/install.sh | sh && sudo systemctl enable --now tailscaled && sudo tailscale up' } })}
              className="flex items-center gap-2 px-4 py-2 rounded-lg text-sm font-medium transition-all duration-200 hover:opacity-90"
              style={{ backgroundColor: 'var(--accent)', color: '#ffffff' }}
            >
              <TerminalSquare size={16} />
              Instalar Tailscale
            </button>
            <button
              onClick={() => navigate('/terminal', { state: { commands: 'tailscale status' } })}
              className="flex items-center gap-2 px-4 py-2 rounded-lg text-sm font-medium transition-all duration-200 hover:opacity-90"
              style={{ color: 'var(--accent)', border: '1px solid var(--accent)' }}
            >
              Ver estado
            </button>
          </div>
        </div>
      </section>

      {/* mDNS - Local domain (admin only) */}
      <section>
        <div className="flex items-center gap-3 mb-4">
          <Globe size={22} style={{ color: 'var(--accent)' }} />
          <h2 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>
            Nombre Local (.local)
          </h2>
        </div>
        <div
          className="rounded-xl p-6"
          style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}
        >
          <p className="text-xs mb-4" style={{ color: 'var(--text-secondary)' }}>
            Permite acceder a LabNAS como <strong style={{ color: 'var(--accent)' }}>{mdnsHostname || 'labnas'}.local:3001</strong> desde cualquier dispositivo en la red, sin recordar la IP.
          </p>
          <div className="flex items-center gap-3 flex-wrap">
            <input
              type="text"
              value={mdnsHostname}
              onChange={e => setMdnsHostname(e.target.value.toLowerCase().replace(/[^a-z0-9-]/g, ''))}
              placeholder="labnas"
              className="px-3 py-2 rounded-lg text-sm outline-none font-mono w-40"
              style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
            />
            <span className="text-sm" style={{ color: 'var(--text-secondary)' }}>.local:3001</span>
            <button
              onClick={async () => {
                try {
                  const result = await setMdns(!mdns?.enabled, mdnsHostname)
                  setMdnsState(result)
                } catch {}
              }}
              className="px-4 py-2 rounded-lg text-sm font-medium"
              style={{
                backgroundColor: mdns?.enabled ? 'var(--danger)' + '20' : 'var(--accent)',
                color: mdns?.enabled ? 'var(--danger)' : '#ffffff',
                border: mdns?.enabled ? '1px solid var(--danger)' : 'none',
              }}
            >
              {mdns?.enabled ? 'Desactivar' : 'Activar'}
            </button>
            {mdns?.enabled && (
              <span className="text-xs px-2 py-0.5 rounded-full font-medium" style={{ backgroundColor: 'var(--success)' + '25', color: 'var(--success)' }}>
                Activo
              </span>
            )}
          </div>
        </div>
      </section>
    </>
  )
}

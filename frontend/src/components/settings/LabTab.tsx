import { useEffect, useState } from 'react'
import { Building2, ExternalLink, Plus, Pencil, Trash2 } from 'lucide-react'
import { themes, getThemeNames, type ThemeName } from '../../themes/themes'
import { getBranding, setBranding, getServices, addService, deleteService, updateService, fetchHealth, type LabBranding, type LabService } from '../../api'
import { errorMessage } from '../../lib/errors'
import DisplayScreensSection from './DisplayScreensSection'

export default function LabTab() {
  const [branding, setBrandingState] = useState<LabBranding | null>(null)
  const [services, setServices] = useState<LabService[]>([])
  const [showAddService, setShowAddService] = useState(false)
  const [svcName, setSvcName] = useState('')
  const [svcPort, setSvcPort] = useState('')
  const [svcDesc, setSvcDesc] = useState('')
  const [svcIcon, setSvcIcon] = useState('')
  const [editingSvcPort, setEditingSvcPort] = useState<number | null>(null)
  const [serverIp, setServerIp] = useState<string | null>(null)

  useEffect(() => {
    getBranding().then(setBrandingState).catch(() => {})
    getServices().then(setServices).catch(() => {})
    fetchHealth().then(h => { if (h.ip) setServerIp(h.ip) }).catch(() => {})
  }, [])

  return (
    <>
      {/* Lab Branding (admin) */}
      {branding && (
        <section>
          <div className="flex items-center gap-3 mb-4">
            <Building2 size={22} style={{ color: 'var(--accent)' }} />
            <h2 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>
              Identidad del Laboratorio
            </h2>
          </div>
          <div
            className="rounded-xl p-6 space-y-4"
            style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}
          >
            <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
              <div>
                <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Nombre del laboratorio</label>
                <input value={branding.lab_name} onChange={e => setBrandingState({ ...branding, lab_name: e.target.value })}
                  placeholder="FabLab Valdivia" className="w-full px-3 py-2 rounded-lg text-sm outline-none"
                  style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }} />
              </div>
              <div>
                <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Institucion</label>
                <input value={branding.institution} onChange={e => setBrandingState({ ...branding, institution: e.target.value })}
                  placeholder="Universidad Austral de Chile" className="w-full px-3 py-2 rounded-lg text-sm outline-none"
                  style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }} />
              </div>
              <div>
                <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>URL del logo</label>
                <input value={branding.logo_url} onChange={e => setBrandingState({ ...branding, logo_url: e.target.value })}
                  placeholder="https://ejemplo.com/logo.png" className="w-full px-3 py-2 rounded-lg text-sm outline-none font-mono"
                  style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }} />
              </div>
              <div>
                <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Sitio web</label>
                <input value={branding.website} onChange={e => setBrandingState({ ...branding, website: e.target.value })}
                  placeholder="https://fablab.uach.cl" className="w-full px-3 py-2 rounded-lg text-sm outline-none font-mono"
                  style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }} />
              </div>
              <div>
                <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Email de contacto</label>
                <input value={branding.contact_email} onChange={e => setBrandingState({ ...branding, contact_email: e.target.value })}
                  placeholder="fablab@uach.cl" className="w-full px-3 py-2 rounded-lg text-sm outline-none"
                  style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }} />
              </div>
              <div>
                <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Ubicacion</label>
                <input value={branding.location} onChange={e => setBrandingState({ ...branding, location: e.target.value })}
                  placeholder="Valdivia, Chile" className="w-full px-3 py-2 rounded-lg text-sm outline-none"
                  style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }} />
              </div>
            </div>
            <div>
              <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Mision</label>
              <textarea value={branding.mission} onChange={e => setBrandingState({ ...branding, mission: e.target.value })}
                rows={2} placeholder="Promover la fabricacion digital y la innovacion..."
                className="w-full px-3 py-2 rounded-lg text-sm outline-none resize-none"
                style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }} />
            </div>
            <div>
              <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Vision</label>
              <textarea value={branding.vision} onChange={e => setBrandingState({ ...branding, vision: e.target.value })}
                rows={2} placeholder="Ser un referente en fabricacion digital en la region..."
                className="w-full px-3 py-2 rounded-lg text-sm outline-none resize-none"
                style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }} />
            </div>
            {/* Color principal */}
            <div>
              <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Color principal (accent)</label>
              <div className="flex items-center gap-3">
                <input
                  type="color"
                  value={branding.accent_color || '#bd93f9'}
                  onChange={e => setBrandingState({ ...branding, accent_color: e.target.value })}
                  className="w-10 h-10 rounded-lg cursor-pointer border-0 p-0"
                  style={{ backgroundColor: 'var(--input-bg)' }}
                />
                <span className="text-xs font-mono" style={{ color: 'var(--text-secondary)' }}>
                  {branding.accent_color || 'Sin personalizar (usa el del tema)'}
                </span>
                {branding.accent_color && (
                  <button
                    onClick={() => {
                      setBrandingState({ ...branding, accent_color: '' })
                      localStorage.removeItem('labnas-accent-color')
                      // Reaplicar tema sin accent personalizado
                      const currentTheme = localStorage.getItem('labnas-theme') || 'dracula'
                      const validThemes = getThemeNames()
                      const resolved = validThemes.includes(currentTheme as ThemeName) ? currentTheme as ThemeName : 'dracula'
                      const themeColors = themes[resolved]
                      document.documentElement.style.setProperty('--accent', themeColors.accent)
                      document.documentElement.style.setProperty('--accent-alpha', themeColors['accent-alpha'])
                      document.documentElement.style.setProperty('--sidebar-active', themeColors['sidebar-active'])
                    }}
                    className="text-xs px-2 py-1 rounded-lg transition-opacity hover:opacity-80"
                    style={{ color: 'var(--danger)', border: '1px solid var(--danger)' }}
                  >
                    Quitar
                  </button>
                )}
              </div>
            </div>

            <button
              onClick={async () => {
                try {
                  await setBranding(branding)
                  // Aplicar accent color inmediatamente
                  if (branding.accent_color) {
                    localStorage.setItem('labnas-accent-color', branding.accent_color)
                    document.documentElement.style.setProperty('--accent', branding.accent_color)
                    document.documentElement.style.setProperty('--accent-alpha', branding.accent_color + '1f')
                    document.documentElement.style.setProperty('--sidebar-active', branding.accent_color)
                  } else {
                    localStorage.removeItem('labnas-accent-color')
                  }
                } catch {}
              }}
              className="px-4 py-2 rounded-lg text-sm font-medium"
              style={{ backgroundColor: 'var(--accent)', color: '#ffffff' }}
            >
              Guardar
            </button>
          </div>
        </section>
      )}

      {/* Servicios del Lab (admin) */}
      <section>
        <div className="flex items-center justify-between mb-4">
          <div className="flex items-center gap-3">
            <ExternalLink size={22} style={{ color: 'var(--accent)' }} />
            <h2 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>Servicios del Lab</h2>
          </div>
          <button
            onClick={() => { setShowAddService(true); setEditingSvcPort(null); setSvcName(''); setSvcPort(''); setSvcDesc(''); setSvcIcon('') }}
            className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium transition-all hover:opacity-90"
            style={{ backgroundColor: 'var(--accent)', color: '#fff' }}
          >
            <Plus size={14} />
            Agregar servicio
          </button>
        </div>

        {showAddService && (
          <div className="rounded-xl p-5 mb-4 space-y-3" style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}>
            <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
              <div>
                <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Nombre</label>
                <input value={svcName} onChange={e => setSvcName(e.target.value)}
                  placeholder="CVAT" className="w-full px-3 py-2 rounded-lg text-sm outline-none"
                  style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }} />
              </div>
              <div>
                <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Puerto</label>
                <input type="number" value={svcPort} onChange={e => setSvcPort(e.target.value)}
                  placeholder="8080" className="w-full px-3 py-2 rounded-lg text-sm outline-none"
                  style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }} />
              </div>
              <div>
                <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Descripcion (opcional)</label>
                <input value={svcDesc} onChange={e => setSvcDesc(e.target.value)}
                  placeholder="Anotacion de imagenes" className="w-full px-3 py-2 rounded-lg text-sm outline-none"
                  style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }} />
              </div>
              <div>
                <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Icono emoji (opcional)</label>
                <input value={svcIcon} onChange={e => setSvcIcon(e.target.value)}
                  placeholder="🏷️" className="w-full px-3 py-2 rounded-lg text-sm outline-none"
                  style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }} />
              </div>
            </div>
            <div className="flex items-center gap-2">
              <button
                onClick={async () => {
                  if (!svcName || !svcPort) return
                  try {
                    if (editingSvcPort !== null) {
                      await updateService(editingSvcPort, { name: svcName, port: parseInt(svcPort), description: svcDesc, icon: svcIcon })
                      setServices(await getServices())
                    } else {
                      const updated = await addService({ name: svcName, port: parseInt(svcPort), description: svcDesc, icon: svcIcon })
                      setServices(updated)
                    }
                    setSvcName(''); setSvcPort(''); setSvcDesc(''); setSvcIcon(''); setEditingSvcPort(null)
                    setShowAddService(false)
                  } catch (e) { alert(errorMessage(e)) }
                }}
                disabled={!svcName || !svcPort}
                className="px-4 py-2 rounded-lg text-sm font-medium"
                style={{ backgroundColor: 'var(--accent)', color: '#fff' }}>
                {editingSvcPort !== null ? 'Guardar' : 'Agregar'}
              </button>
              <button onClick={() => setShowAddService(false)}
                className="px-4 py-2 rounded-lg text-sm font-medium"
                style={{ color: 'var(--text-secondary)', border: '1px solid var(--border)' }}>
                Cancelar
              </button>
            </div>
          </div>
        )}

        <div className="rounded-xl overflow-hidden" style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}>
          {services.length === 0 ? (
            <div className="text-center py-8">
              <ExternalLink size={32} className="mx-auto mb-2" style={{ color: 'var(--text-secondary)', opacity: 0.4 }} />
              <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>No hay servicios configurados</p>
            </div>
          ) : (
            <div className="divide-y" style={{ borderColor: 'var(--border)' }}>
              {services.map(svc => {
                const svcHost = (serverIp && window.location.hostname.includes('.local')) ? serverIp : window.location.hostname
                const svcUrl = `${window.location.protocol}//${svcHost}:${svc.port}`
                return (
                  <div key={svc.port} className="flex items-center justify-between px-5 py-3">
                    <div className="flex items-center gap-3 min-w-0">
                      <span className="text-lg">{svc.icon || '🔗'}</span>
                      <div className="min-w-0">
                        <span className="text-sm font-medium" style={{ color: 'var(--text-primary)' }}>{svc.name}</span>
                        <span className="text-xs font-mono ml-2" style={{ color: 'var(--accent)' }}>:{svc.port}</span>
                        {svc.description && (
                          <span className="text-xs ml-2" style={{ color: 'var(--text-secondary)' }}>{svc.description}</span>
                        )}
                        <a
                          href={svcUrl}
                          target="_blank"
                          rel="noopener noreferrer"
                          className="block text-[10px] font-mono truncate hover:underline"
                          style={{ color: 'var(--text-secondary)', opacity: 0.7 }}
                        >
                          {svcUrl}
                        </a>
                      </div>
                    </div>
                    <div className="flex items-center gap-1.5">
                      <a
                        href={svcUrl}
                        target="_blank"
                        rel="noopener noreferrer"
                        className="p-1.5 rounded-lg transition-all hover:opacity-80"
                        style={{ color: 'var(--accent)' }}
                        title="Abrir servicio"
                      >
                        <ExternalLink size={14} />
                      </a>
                      <button onClick={() => {
                        setSvcName(svc.name); setSvcPort(String(svc.port)); setSvcDesc(svc.description); setSvcIcon(svc.icon)
                        setEditingSvcPort(svc.port); setShowAddService(true)
                      }}
                        className="p-1.5 rounded-lg transition-all hover:opacity-80" style={{ color: 'var(--text-secondary)' }}
                        title="Editar">
                        <Pencil size={14} />
                      </button>
                      <button onClick={async () => {
                        try {
                          await deleteService(svc.port)
                          setServices(prev => prev.filter(s => s.port !== svc.port))
                        } catch {}
                      }}
                        className="p-1.5 rounded-lg transition-all hover:opacity-80" style={{ color: 'var(--danger)' }}>
                        <Trash2 size={14} />
                      </button>
                    </div>
                  </div>
                )
              })}
            </div>
          )}
        </div>
      </section>

      <DisplayScreensSection />
    </>
  )
}

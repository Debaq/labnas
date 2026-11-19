import { useEffect, useState } from 'react'
import { KeyRound, Loader2 } from 'lucide-react'
import { fetchMsOAuthConfig, saveMsOAuthConfig } from '../api'
import { errorMessage } from '../lib/errors'

const inputStyle = { backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }
const TENANTS: Record<string, string> = {
  common: 'Cuentas personales y de organización',
  organizations: 'Solo cuentas de organización (Microsoft 365)',
  consumers: 'Solo cuentas personales (Outlook.com, Hotmail)',
}

/**
 * Apps publicas ya registradas en Microsoft que sirven para leer correo por IMAP con
 * codigo de dispositivo. Agregar aqui solo las que se hayan probado de verdad.
 * (Evolution de GNOME no sirve: su app esta habilitada para EWS, no para IMAP.)
 */
const KNOWN_APPS: { id: string; name: string; note: string }[] = [
  {
    id: '9e5f94bc-e8a4-4e73-b8be-63364c29d753',
    name: 'Thunderbird (Mozilla)',
    note: 'Probado con Microsoft 365 de la UACh. Microsoft mostrará «Mozilla Thunderbird» al iniciar sesión.',
  },
]
const CUSTOM = 'propia'

/** Admin: app de Microsoft con la que los usuarios conectan su correo */
export default function MicrosoftOAuthSection() {
  const [clientId, setClientId] = useState('')
  const [app, setApp] = useState(CUSTOM)
  const [tenant, setTenant] = useState('common')
  const [customTenant, setCustomTenant] = useState('')
  const [busy, setBusy] = useState(false)
  const [msg, setMsg] = useState<{ ok: boolean; text: string } | null>(null)

  useEffect(() => {
    fetchMsOAuthConfig().then(c => {
      setClientId(c.client_id)
      setApp(KNOWN_APPS.some(a => a.id === c.client_id) ? c.client_id : CUSTOM)
      if (c.tenant in TENANTS) setTenant(c.tenant)
      else { setTenant('custom'); setCustomTenant(c.tenant) }
    }).catch(e => setMsg({ ok: false, text: errorMessage(e) }))
  }, [])

  async function save() {
    setBusy(true)
    setMsg(null)
    try {
      const t = tenant === 'custom' ? customTenant.trim() : tenant
      const id = app === CUSTOM ? clientId.trim() : app
      const c = await saveMsOAuthConfig({ client_id: id, tenant: t })
      setMsg({ ok: true, text: c.configured ? 'Guardado: los usuarios ya pueden conectar cuentas Microsoft' : 'Guardado (sin Client ID: conexión con Microsoft desactivada)' })
    } catch (e) {
      setMsg({ ok: false, text: errorMessage(e) })
    } finally {
      setBusy(false)
    }
  }

  return (
    <section>
      <div className="flex items-center gap-3 mb-4">
        <KeyRound size={22} style={{ color: 'var(--accent)' }} />
        <h2 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>Conexión con Microsoft</h2>
      </div>
      <div className="rounded-xl p-6 space-y-4" style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}>
        <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>
          Outlook.com, Hotmail y Microsoft 365 ya no aceptan contraseña por IMAP: cada usuario inicia sesión en Microsoft
          y autoriza a una «aplicación» a leer su correo. Puedes usar una app de correo conocida que ya está registrada en
          Microsoft (no requiere permisos especiales) o una propia, si TI de tu institución la registró.
        </p>
        <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
          <div>
            <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Aplicación</label>
            <select value={app} onChange={e => setApp(e.target.value)} className="w-full px-3 py-2 rounded-lg text-sm outline-none cursor-pointer" style={inputStyle}>
              {KNOWN_APPS.map(a => <option key={a.id} value={a.id}>{a.name}</option>)}
              <option value={CUSTOM}>App propia (Client ID registrado por TI)</option>
            </select>
            {app === CUSTOM ? (
              <input value={clientId} onChange={e => setClientId(e.target.value)} placeholder="Client ID: xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
                className="w-full mt-2 px-3 py-2 rounded-lg text-sm outline-none font-mono" style={inputStyle} />
            ) : (
              <p className="mt-2 text-xs" style={{ color: 'var(--text-secondary)' }}>
                {KNOWN_APPS.find(a => a.id === app)?.note}
                <span className="block font-mono opacity-70">{app}</span>
              </p>
            )}
          </div>
          <div>
            <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Cuentas permitidas (tenant)</label>
            <select value={tenant} onChange={e => setTenant(e.target.value)} className="w-full px-3 py-2 rounded-lg text-sm outline-none cursor-pointer" style={inputStyle}>
              {Object.entries(TENANTS).map(([k, v]) => <option key={k} value={k}>{v}</option>)}
              <option value="custom">Un tenant específico (dominio o ID)</option>
            </select>
            {tenant === 'custom' && (
              <input value={customTenant} onChange={e => setCustomTenant(e.target.value)} placeholder="uach.cl"
                className="w-full mt-2 px-3 py-2 rounded-lg text-sm outline-none font-mono" style={inputStyle} />
            )}
          </div>
        </div>
        <button onClick={save} disabled={busy || (tenant === 'custom' && !customTenant.trim())}
          className="flex items-center gap-2 px-4 py-2 rounded-lg text-sm font-medium transition-all hover:opacity-90"
          style={{ backgroundColor: 'var(--accent)', color: '#fff' }}>
          {busy && <Loader2 size={16} className="animate-spin" />}Guardar
        </button>
        {msg && <p className="text-xs" style={{ color: msg.ok ? 'var(--success)' : 'var(--danger)' }}>{msg.text}</p>}

        <details className="text-xs" style={{ color: 'var(--text-secondary)' }}>
          <summary className="cursor-pointer font-medium" style={{ color: 'var(--text-primary)' }}>App propia: cómo registrarla en Microsoft Entra (requiere acceso de administrador de la institución)</summary>
          <ol className="list-decimal list-inside space-y-1.5 mt-2">
            <li>Entra a <a href="https://entra.microsoft.com" target="_blank" rel="noopener noreferrer" className="underline">entra.microsoft.com</a> &gt; Identidad &gt; Aplicaciones &gt; Registros de aplicaciones &gt; <em>Nuevo registro</em>. Nombre: LabNAS. Tipos de cuenta: «Cuentas en cualquier directorio organizativo y cuentas personales de Microsoft». Sin URI de redirección.</li>
            <li>En la app: <em>Autenticación</em> &gt; Configuración avanzada &gt; «Permitir flujos de clientes públicos» = Sí &gt; Guardar.</li>
            <li><em>Permisos de API</em> &gt; Agregar un permiso &gt; «API que usa mi organización» &gt; «Office 365 Exchange Online» &gt; Permisos delegados &gt; <code>IMAP.AccessAsUser.All</code>. (El permiso <code>offline_access</code> lo pide LabNAS solo.)</li>
            <li>Copia el «Id. de aplicación (cliente)» de la página Información general y pégalo arriba.</li>
          </ol>
          <p className="mt-2">Si tu institución bloquea apps de terceros, al iniciar sesión Microsoft pedirá la aprobación de un administrador: pide a TI que apruebe la app «LabNAS» (solo lectura de correo por IMAP).</p>
        </details>
      </div>
    </section>
  )
}

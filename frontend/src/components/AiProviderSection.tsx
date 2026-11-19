import { useEffect, useState } from 'react'
import { Key, Loader2, RefreshCw, Trash2 } from 'lucide-react'
import { fetchAiConfig, fetchAiModels, saveAiConfig, testAi, type AiConfig } from '../api'
import { errorMessage } from '../lib/errors'

const PRESETS: Record<string, { label: string; url: string; model: string }> = {
  deepseek: { label: 'DeepSeek', url: 'https://api.deepseek.com', model: 'deepseek-chat' },
  groq: { label: 'Groq', url: 'https://api.groq.com/openai/v1', model: 'llama-3.3-70b-versatile' },
  openai: { label: 'OpenAI', url: 'https://api.openai.com/v1', model: 'gpt-4o-mini' },
  ollama: { label: 'Ollama local', url: 'http://localhost:11434/v1', model: 'llama3.1:8b' },
  custom: { label: 'Personalizado', url: '', model: '' },
}

function presetFor(url: string, model: string): string {
  const k = Object.keys(PRESETS).find(k => k !== 'custom' && PRESETS[k].url === url && PRESETS[k].model === model)
  return k ?? 'custom'
}

const inputStyle = { backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }

export default function AiProviderSection() {
  const [cfg, setCfg] = useState<AiConfig | null>(null)
  const [preset, setPreset] = useState('deepseek')
  const [baseUrl, setBaseUrl] = useState('')
  const [model, setModel] = useState('')
  const [apiKey, setApiKey] = useState('')
  const [busy, setBusy] = useState(false)
  const [testing, setTesting] = useState(false)
  const [msg, setMsg] = useState<{ ok: boolean; text: string } | null>(null)
  const [models, setModels] = useState<string[] | null>(null)
  const [loadingModels, setLoadingModels] = useState(false)

  async function loadModels(url: string, key?: string, quiet = false) {
    if (!url.trim()) return
    setLoadingModels(true)
    try {
      setModels(await fetchAiModels(url.trim(), key?.trim()))
    } catch (e) {
      setModels(null)
      if (!quiet) setMsg({ ok: false, text: errorMessage(e) })
    } finally {
      setLoadingModels(false)
    }
  }

  function apply(c: AiConfig) {
    setCfg(c)
    setBaseUrl(c.base_url)
    setModel(c.model)
    setPreset(c.base_url ? presetFor(c.base_url, c.model) : 'deepseek')
  }

  useEffect(() => {
    fetchAiConfig().then(c => {
      if (!c.base_url) { setCfg(c); setBaseUrl(PRESETS.deepseek.url); setModel(PRESETS.deepseek.model) }
      else {
        apply(c)
        // Ya configurada: traer la lista de modelos del proveedor sin pedir nada
        if (c.enabled) loadModels(c.base_url, undefined, true)
      }
    }).catch(e => setMsg({ ok: false, text: errorMessage(e) }))
  }, [])

  function choosePreset(k: string) {
    setPreset(k)
    if (k !== 'custom') { setBaseUrl(PRESETS[k].url); setModel(PRESETS[k].model) }
    // Otro proveedor, otra lista
    setModels(null)
  }

  async function save(key?: string) {
    setBusy(true)
    setMsg(null)
    try {
      const body: { base_url: string; model: string; api_key?: string } = { base_url: baseUrl.trim(), model: model.trim() }
      if (key !== undefined) body.api_key = key
      else if (apiKey.trim()) body.api_key = apiKey.trim()
      apply(await saveAiConfig(body))
      setApiKey('')
      setMsg({ ok: true, text: key === '' ? 'Clave borrada' : 'Configuración guardada' })
    } catch (e) {
      setMsg({ ok: false, text: errorMessage(e) })
    } finally {
      setBusy(false)
    }
  }

  async function test() {
    setTesting(true)
    setMsg(null)
    try {
      const r = await testAi()
      setMsg({ ok: r.ok, text: r.ok ? `Conexión OK (${r.model}, ${r.ms} ms)` : 'La prueba falló' })
    } catch (e) {
      setMsg({ ok: false, text: errorMessage(e) })
    } finally {
      setTesting(false)
    }
  }

  return (
    <section>
      <div className="flex items-center gap-3 mb-4">
        <Key size={22} style={{ color: 'var(--accent)' }} />
        <h2 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>Proveedor de IA</h2>
      </div>
      <div className="rounded-xl p-6 space-y-4" style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}>
        <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>
          Una sola configuración para todo LabNAS (hoy la usa el correo; la usarán otros módulos). Cualquier servicio compatible con la API de OpenAI. La IA solo propone: una persona aprueba.
        </p>
        <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
          <div>
            <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Preset</label>
            <select value={preset} onChange={e => choosePreset(e.target.value)} className="w-full px-3 py-2 rounded-lg text-sm outline-none cursor-pointer" style={inputStyle}>
              {Object.entries(PRESETS).map(([k, v]) => <option key={k} value={k}>{v.label}</option>)}
            </select>
          </div>
          <div>
            <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Modelo</label>
            <div className="flex items-center gap-2">
              {models ? (
                <select value={model} onChange={e => setModel(e.target.value)}
                  className="flex-1 min-w-0 px-3 py-2 rounded-lg text-sm outline-none cursor-pointer font-mono" style={inputStyle}>
                  {!models.includes(model) && <option value={model}>{model || '(elige un modelo)'} — no está en la lista</option>}
                  {models.map(m => <option key={m} value={m}>{m}</option>)}
                </select>
              ) : (
                <input value={model} onChange={e => { setModel(e.target.value); setPreset('custom') }}
                  className="flex-1 min-w-0 px-3 py-2 rounded-lg text-sm outline-none font-mono" style={inputStyle} />
              )}
              <button onClick={() => loadModels(baseUrl, apiKey)} disabled={loadingModels || !baseUrl.trim()}
                title="Pedir al proveedor la lista de modelos disponibles (usa la clave escrita o la guardada)"
                className="flex items-center gap-1 px-3 py-2 rounded-lg text-xs font-medium whitespace-nowrap"
                style={{ color: 'var(--text-primary)', border: '1px solid var(--border)' }}>
                {loadingModels ? <Loader2 size={14} className="animate-spin" /> : <RefreshCw size={14} />}
                {models ? 'Recargar' : 'Cargar modelos'}
              </button>
            </div>
            {models && !models.includes(model) && model && (
              <p className="mt-1 text-xs" style={{ color: 'var(--warning)' }}>
                El proveedor no ofrece «{model}»: elige uno de la lista y guarda.
              </p>
            )}
          </div>
          <div>
            <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>URL base</label>
            <input value={baseUrl} onChange={e => { setBaseUrl(e.target.value); setPreset('custom'); setModels(null) }} className="w-full px-3 py-2 rounded-lg text-sm outline-none font-mono" style={inputStyle} />
          </div>
          <div>
            <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Clave API</label>
            <div className="flex items-center gap-2">
              <input type="password" autoComplete="new-password" value={apiKey} onChange={e => setApiKey(e.target.value)}
                placeholder={cfg?.has_key ? '•••• guardada' : 'sk-...'} className="flex-1 min-w-0 px-3 py-2 rounded-lg text-sm outline-none font-mono" style={inputStyle} />
              {cfg?.has_key && (
                <button onClick={() => save('')} disabled={busy} title="Borrar clave"
                  className="flex items-center gap-1 px-3 py-2 rounded-lg text-xs font-medium whitespace-nowrap"
                  style={{ color: 'var(--danger)', border: '1px solid var(--danger)' }}>
                  <Trash2 size={14} />Borrar clave
                </button>
              )}
            </div>
          </div>
        </div>
        <div className="flex items-center gap-2 flex-wrap">
          <button onClick={() => save()} disabled={busy || !baseUrl.trim() || !model.trim()}
            className="flex items-center gap-2 px-4 py-2 rounded-lg text-sm font-medium transition-all hover:opacity-90"
            style={{ backgroundColor: 'var(--accent)', color: '#fff' }}>
            {busy && <Loader2 size={16} className="animate-spin" />}Guardar
          </button>
          <button onClick={test} disabled={testing || busy}
            className="flex items-center gap-2 px-4 py-2 rounded-lg text-sm font-medium"
            style={{ color: 'var(--text-primary)', border: '1px solid var(--border)' }}>
            {testing && <Loader2 size={16} className="animate-spin" />}Probar conexión
          </button>
        </div>
        {msg && <p className="text-xs" style={{ color: msg.ok ? 'var(--success)' : 'var(--danger)' }}>{msg.text}</p>}
        <p className="text-xs" style={{ color: 'var(--text-secondary)', opacity: 0.8 }}>
          Con un proveedor en la nube, lo que se le pide a la IA (por ejemplo, el texto de los correos) se envía a ese servicio. Con Ollama local no sale de tu red.
        </p>
      </div>
    </section>
  )
}

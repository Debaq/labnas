import { useEffect, useState } from 'react'
import { Radio, Music, HelpCircle, X } from 'lucide-react'
import { setLastfmKey, getMpvArgs, setMpvArgs as saveMpvArgs } from '../../../api'

export default function MusicSettings() {
  const [lastfmKey, setLastfmKeyInput] = useState('')
  const [lastfmMsg, setLastfmMsg] = useState<{ ok: boolean; text: string } | null>(null)
  const [mpvArgs, setMpvArgs] = useState<string[]>([])
  const [mpvArgsInput, setMpvArgsInput] = useState('')
  const [mpvMsg, setMpvMsg] = useState<{ ok: boolean; text: string } | null>(null)
  const [showMpvHelp, setShowMpvHelp] = useState(false)

  useEffect(() => {
    getMpvArgs().then(args => { setMpvArgs(args); setMpvArgsInput(args.join('\n')) }).catch(() => {})
  }, [])

  return (
    <>
      {/* Last.fm (admin only) */}
      <section>
        <div className="flex items-center gap-3 mb-4">
          <Radio size={22} style={{ color: 'var(--accent)' }} />
          <h2 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>
            Last.fm Radio
          </h2>
        </div>
        <div className="rounded-xl p-5 space-y-3" style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}>
          <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>
            API key de Last.fm para buscar canciones similares. Obtener en <a href="https://www.last.fm/api/account/create" target="_blank" rel="noopener noreferrer" className="underline" style={{ color: 'var(--accent)' }}>last.fm/api</a>
          </p>
          <div className="flex gap-2">
            <input
              type="text"
              value={lastfmKey}
              onChange={(e) => setLastfmKeyInput(e.target.value)}
              placeholder="API key de Last.fm"
              className="flex-1 px-3 py-2 rounded-lg text-sm outline-none font-mono"
              style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
            />
            <button
              onClick={async () => {
                if (!lastfmKey.trim()) return
                try {
                  await setLastfmKey(lastfmKey.trim())
                  setLastfmMsg({ ok: true, text: 'API key guardada' })
                  setLastfmKeyInput('')
                } catch {
                  setLastfmMsg({ ok: false, text: 'Error guardando key' })
                }
                setTimeout(() => setLastfmMsg(null), 3000)
              }}
              disabled={!lastfmKey.trim()}
              className="px-4 py-2 rounded-lg text-sm font-medium"
              style={{ backgroundColor: 'var(--accent)', color: '#ffffff' }}
            >
              Guardar
            </button>
          </div>
          {lastfmMsg && (
            <p className="text-xs" style={{ color: lastfmMsg.ok ? 'var(--success)' : 'var(--danger)' }}>
              {lastfmMsg.text}
            </p>
          )}
        </div>
      </section>

      {/* mpv Args (admin only) */}
      <section>
        <div className="flex items-center gap-3 mb-4">
          <Music size={22} style={{ color: 'var(--accent)' }} />
          <h2 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>
            Reproductor (mpv)
          </h2>
          <button
            onClick={() => setShowMpvHelp(true)}
            className="ml-auto p-1.5 rounded-lg hover:opacity-80 transition-opacity"
            style={{ color: 'var(--text-secondary)' }}
            title="Ayuda"
          >
            <HelpCircle size={18} />
          </button>
        </div>
        <div className="rounded-xl p-5 space-y-3" style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}>
          <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>
            Argumentos extra que se pasan a mpv al reproducir. Un argumento por linea.
          </p>
          <textarea
            value={mpvArgsInput}
            onChange={(e) => setMpvArgsInput(e.target.value)}
            placeholder={'--af=lavfi=[loudnorm=I=-14:TP=-1:LRA=11]\n--audio-pitch-correction=yes'}
            rows={4}
            className="w-full px-3 py-2 rounded-lg text-sm outline-none font-mono resize-y"
            style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
          />
          <div className="flex items-center gap-2">
            <button
              onClick={async () => {
                try {
                  const args = mpvArgsInput.split('\n').map(l => l.trim()).filter(l => l.length > 0)
                  const saved = await saveMpvArgs(args)
                  setMpvArgs(saved)
                  setMpvMsg({ ok: true, text: 'Guardado. Se aplicara en la proxima cancion.' })
                } catch {
                  setMpvMsg({ ok: false, text: 'Error guardando args' })
                }
                setTimeout(() => setMpvMsg(null), 3000)
              }}
              className="px-4 py-2 rounded-lg text-sm font-medium"
              style={{ backgroundColor: 'var(--accent)', color: '#ffffff' }}
            >
              Guardar
            </button>
            {mpvArgs.length > 0 && (
              <span className="text-xs" style={{ color: 'var(--text-secondary)' }}>
                {mpvArgs.length} arg{mpvArgs.length !== 1 ? 's' : ''} activo{mpvArgs.length !== 1 ? 's' : ''}
              </span>
            )}
          </div>
          {mpvMsg && (
            <p className="text-xs" style={{ color: mpvMsg.ok ? 'var(--success)' : 'var(--danger)' }}>
              {mpvMsg.text}
            </p>
          )}
        </div>
      </section>

      {/* Modal ayuda mpv */}
      {showMpvHelp && (
        <div className="fixed inset-0 z-50 flex items-center justify-center p-4" style={{ backgroundColor: 'rgba(0,0,0,0.5)' }}>
          <div className="rounded-2xl p-6 max-w-lg w-full max-h-[80vh] overflow-y-auto space-y-4" style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}>
            <div className="flex items-center justify-between">
              <h3 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>Argumentos de mpv</h3>
              <button onClick={() => setShowMpvHelp(false)} className="p-1 rounded-lg hover:opacity-80" style={{ color: 'var(--text-secondary)' }}>
                <X size={20} />
              </button>
            </div>
            <p className="text-sm" style={{ color: 'var(--text-secondary)' }}>
              Estos argumentos se pasan directamente a mpv cada vez que se reproduce una cancion. Un argumento por linea.
            </p>
            <div className="space-y-3">
              <div className="rounded-lg p-3" style={{ backgroundColor: 'var(--bg-secondary)' }}>
                <p className="text-xs font-semibold mb-1" style={{ color: 'var(--accent)' }}>Normalizar volumen (recomendado)</p>
                <code className="text-xs font-mono block" style={{ color: 'var(--text-primary)' }}>--af=lavfi=[loudnorm=I=-14:TP=-1:LRA=11]</code>
                <p className="text-xs mt-1" style={{ color: 'var(--text-secondary)' }}>
                  Iguala el volumen entre canciones usando el estandar EBU R128. Evita saltos de volumen entre tracks.
                </p>
              </div>
              <div className="rounded-lg p-3" style={{ backgroundColor: 'var(--bg-secondary)' }}>
                <p className="text-xs font-semibold mb-1" style={{ color: 'var(--accent)' }}>Corrección de pitch</p>
                <code className="text-xs font-mono block" style={{ color: 'var(--text-primary)' }}>--audio-pitch-correction=yes</code>
                <p className="text-xs mt-1" style={{ color: 'var(--text-secondary)' }}>
                  Mantiene el tono original al cambiar velocidad de reproduccion.
                </p>
              </div>
              <div className="rounded-lg p-3" style={{ backgroundColor: 'var(--bg-secondary)' }}>
                <p className="text-xs font-semibold mb-1" style={{ color: 'var(--accent)' }}>Velocidad de reproduccion</p>
                <code className="text-xs font-mono block" style={{ color: 'var(--text-primary)' }}>--speed=1.0</code>
                <p className="text-xs mt-1" style={{ color: 'var(--text-secondary)' }}>
                  Cambia la velocidad (0.5 = mitad, 2.0 = doble). Util para podcasts.
                </p>
              </div>
              <div className="rounded-lg p-3" style={{ backgroundColor: 'var(--bg-secondary)' }}>
                <p className="text-xs font-semibold mb-1" style={{ color: 'var(--accent)' }}>Cache de red</p>
                <code className="text-xs font-mono block" style={{ color: 'var(--text-primary)' }}>--cache=yes</code>
                <p className="text-xs mt-1" style={{ color: 'var(--text-secondary)' }}>
                  Activa cache para streams de red. Reduce cortes en conexiones lentas.
                </p>
              </div>
              <div className="rounded-lg p-3" style={{ backgroundColor: 'var(--bg-secondary)' }}>
                <p className="text-xs font-semibold mb-1" style={{ color: 'var(--accent)' }}>Ecualizador basico</p>
                <code className="text-xs font-mono block" style={{ color: 'var(--text-primary)' }}>--af=lavfi=[equalizer=f=1000:t=h:w=200:g=5]</code>
                <p className="text-xs mt-1" style={{ color: 'var(--text-secondary)' }}>
                  Filtro de ecualizacion. f=frecuencia, w=ancho, g=ganancia en dB.
                </p>
              </div>
              <div className="rounded-lg p-3" style={{ backgroundColor: 'var(--bg-secondary)' }}>
                <p className="text-xs font-semibold mb-1" style={{ color: 'var(--accent)' }}>Bass boost</p>
                <code className="text-xs font-mono block" style={{ color: 'var(--text-primary)' }}>--af=lavfi=[bass=g=6]</code>
                <p className="text-xs mt-1" style={{ color: 'var(--text-secondary)' }}>
                  Aumenta los graves. g=ganancia en dB (3-10 es un rango razonable).
                </p>
              </div>
            </div>
            <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>
              Referencia completa: <a href="https://mpv.io/manual/master/" target="_blank" rel="noopener noreferrer" className="underline" style={{ color: 'var(--accent)' }}>mpv.io/manual</a>
            </p>
          </div>
        </div>
      )}
    </>
  )
}

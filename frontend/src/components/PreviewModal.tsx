import { useEffect, useState } from 'react'
import { X, Download, Loader2 } from 'lucide-react'
import { createPreviewToken, type PreviewLinks } from '../api'
import { previewKind } from '../lib/preview'

/** Bytes de texto que se muestran como maximo */
const TEXT_LIMIT = 512 * 1024

interface Props {
  path: string
  name: string
  onClose: () => void
}

export default function PreviewModal({ path, name, onClose }: Props) {
  const kind = previewKind(name)
  const [links, setLinks] = useState<PreviewLinks | null>(null)
  const [text, setText] = useState<{ body: string; truncated: boolean } | null>(null)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    let cancelled = false
    createPreviewToken(path)
      .then(async (l) => {
        if (cancelled) return
        setLinks(l)
        if (kind === 'text') {
          const res = await fetch(l.url, { headers: { Range: `bytes=0-${TEXT_LIMIT - 1}` } })
          const body = await res.text()
          const total = Number(res.headers.get('content-range')?.split('/')[1] ?? body.length)
          if (!cancelled) setText({ body, truncated: total > TEXT_LIMIT })
        }
      })
      .catch((e) => !cancelled && setError(e instanceof Error ? e.message : String(e)))
    return () => { cancelled = true }
  }, [path, kind])

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => { if (e.key === 'Escape') onClose() }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [onClose])

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4" style={{ backgroundColor: 'rgba(0,0,0,0.75)' }} onClick={onClose}>
      <div
        className="w-full max-w-5xl h-[85vh] flex flex-col rounded-xl overflow-hidden"
        style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center justify-between px-4 py-3" style={{ borderBottom: '1px solid var(--border)' }}>
          <span className="text-sm font-medium truncate" style={{ color: 'var(--text-primary)' }}>{name}</span>
          <div className="flex items-center gap-3 shrink-0">
            {links && (
              <a href={links.download_url} download={name} title="Descargar" style={{ color: 'var(--accent)' }}>
                <Download size={18} />
              </a>
            )}
            <button onClick={onClose} title="Cerrar (Esc)" style={{ color: 'var(--text-secondary)' }}><X size={18} /></button>
          </div>
        </div>

        <div className="flex-1 min-h-0 flex items-center justify-center overflow-auto" style={{ backgroundColor: 'var(--bg-primary)' }}>
          {error && <p className="text-sm" style={{ color: 'var(--danger)' }}>{error}</p>}
          {!error && !links && <Loader2 size={24} className="animate-spin" style={{ color: 'var(--text-secondary)' }} />}
          {links && kind === 'image' && <img src={links.url} alt={name} className="max-w-full max-h-full object-contain" />}
          {links && kind === 'video' && <video src={links.url} controls autoPlay className="max-w-full max-h-full" />}
          {links && kind === 'audio' && <audio src={links.url} controls autoPlay className="w-2/3" />}
          {links && kind === 'pdf' && <iframe src={links.url} title={name} className="w-full h-full border-0" />}
          {links && kind === 'text' && (
            text
              ? (
                <div className="w-full h-full overflow-auto">
                  <pre className="p-4 text-xs font-mono whitespace-pre-wrap break-words" style={{ color: 'var(--text-primary)' }}>{text.body}</pre>
                  {text.truncated && (
                    <p className="px-4 pb-4 text-xs" style={{ color: 'var(--warning)' }}>
                      Mostrando los primeros 512 KB. Descarga el archivo para verlo completo.
                    </p>
                  )}
                </div>
              )
              : <Loader2 size={24} className="animate-spin" style={{ color: 'var(--text-secondary)' }} />
          )}
          {links && !kind && <p className="text-sm" style={{ color: 'var(--text-secondary)' }}>Sin vista previa para este tipo de archivo.</p>}
        </div>
      </div>
    </div>
  )
}

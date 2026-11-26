import { useEffect, useMemo, useState } from 'react'
import { Eye, Loader2, Save } from 'lucide-react'
import { PORTAL_SECTIONS, fetchPublicPortal, fetchTransparencyConfig, previewPortal, saveTransparencyConfig, type Portal, type PortalSection, type SectionsState, type TransparencyConfig } from '../../api'
import { useToast } from '../ToastContext'
import { errorMessage } from '../../lib/errors'
import { SECTION_INFO, cardStyle, inputStyle } from '../../lib/transparency'
import PortalView from './PortalView'

/** Que se publica: interruptores por seccion, plazo de las solicitudes y vista previa identica a lo publico */
export default function SectionsPanel({ canManage, onSaved }: { canManage: boolean; onSaved: () => void }) {
  const { addToast } = useToast()
  const [saved, setSaved] = useState<TransparencyConfig | null>(null)
  const [sections, setSections] = useState<SectionsState | null>(null)
  const [days, setDays] = useState(20)
  const [intro, setIntro] = useState('')
  const [preview, setPreview] = useState<Portal | null>(null)
  const [busy, setBusy] = useState(false)

  useEffect(() => {
    fetchTransparencyConfig()
      .then((c) => { setSaved(c); setSections(c.sections); setDays(c.deadline_days); setIntro(c.intro) })
      .catch((e) => addToast(errorMessage(e), 'error'))
  }, [addToast])

  // La vista previa se arma en el servidor con el mismo codigo que la pagina publica
  useEffect(() => {
    if (!sections) return
    const t = setTimeout(() => {
      // Quien solo ve la pagina ve el portal publicado; quien gestiona, el borrador
      const p = canManage ? previewPortal({ sections, deadline_days: days, intro }) : fetchPublicPortal()
      p.then(setPreview).catch(() => setPreview(null))
    }, 250)
    return () => clearTimeout(t)
  }, [sections, days, intro, canManage])

  const dirty = useMemo(
    () => !!saved && !!sections && (PORTAL_SECTIONS.some((s) => saved.sections[s] !== sections[s]) || saved.deadline_days !== days || saved.intro !== intro),
    [saved, sections, days, intro],
  )

  if (!sections) return <Loader2 className="animate-spin" size={20} style={{ color: 'var(--accent)' }} />

  const save = async () => {
    setBusy(true)
    try {
      await saveTransparencyConfig({ sections, deadline_days: days, intro })
      setSaved({ ...(saved as TransparencyConfig), sections, deadline_days: days, intro })
      addToast('Portal actualizado', 'success')
      onSaved()
    } catch (e) {
      addToast(errorMessage(e), 'error')
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="space-y-4">
      <div className="rounded-xl p-4 space-y-3" style={cardStyle}>
        <h2 className="text-sm font-semibold" style={{ color: 'var(--text-primary)' }}>Que se publica</h2>
        <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>
          El portal se arma solo con los datos reales. Elige las secciones; abajo ves exactamente lo que vera el publico en <span className="font-mono">/portal-transparencia</span>.
        </p>
        <div className="space-y-2">
          {PORTAL_SECTIONS.map((s: PortalSection) => (
            <label key={s} className="flex items-start gap-3 cursor-pointer">
              <input type="checkbox" className="mt-1" checked={sections[s]} disabled={!canManage} onChange={(e) => setSections({ ...sections, [s]: e.target.checked })} />
              <span>
                <span className="text-sm font-medium" style={{ color: 'var(--text-primary)' }}>{SECTION_INFO[s].label}</span>
                <span className="block text-xs" style={{ color: 'var(--text-secondary)' }}>{SECTION_INFO[s].hint}</span>
              </span>
            </label>
          ))}
        </div>
        <div className="grid gap-3 sm:grid-cols-2">
          <label className="text-xs" style={{ color: 'var(--text-secondary)' }}>Plazo de respuesta (dias habiles, lunes a viernes)
            <input type="number" min={1} max={90} value={days} disabled={!canManage} onChange={(e) => setDays(Number(e.target.value))} className="block w-full mt-1 px-2 py-1.5 rounded-lg text-sm outline-none" style={inputStyle} />
          </label>
        </div>
        <label className="block text-xs" style={{ color: 'var(--text-secondary)' }}>Texto de introduccion (opcional)
          <textarea value={intro} maxLength={1500} rows={3} disabled={!canManage} onChange={(e) => setIntro(e.target.value)} className="block w-full mt-1 px-2 py-1.5 rounded-lg text-sm outline-none" style={inputStyle} />
        </label>
        {canManage && (
          <button onClick={save} disabled={busy || !dirty} className="inline-flex items-center gap-2 px-3 py-2 rounded-lg text-sm font-medium disabled:opacity-50" style={{ backgroundColor: 'var(--accent)', color: '#fff' }}>
            {busy ? <Loader2 size={14} className="animate-spin" /> : <Save size={14} />} Guardar cambios
          </button>
        )}
        {!canManage && <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>Solo quien gestiona Transparencia puede cambiar esto.</p>}
      </div>

      <div className="space-y-2">
        <h2 className="text-sm font-semibold flex items-center gap-2" style={{ color: 'var(--text-primary)' }}>
          <Eye size={15} /> Vista previa {dirty && <span className="text-xs font-normal" style={{ color: 'var(--warning, #d97706)' }}>(con cambios sin guardar)</span>}
        </h2>
        <div className="rounded-xl p-4" style={{ border: '1px dashed var(--border)' }}>
          {preview ? (
            <>
              <p className="text-base font-semibold mb-3" style={{ color: 'var(--text-primary)' }}>{preview.lab.lab_name} · Portal de transparencia</p>
              <PortalView portal={preview} preview />
            </>
          ) : (
            <Loader2 className="animate-spin" size={18} style={{ color: 'var(--accent)' }} />
          )}
        </div>
      </div>
    </div>
  )
}

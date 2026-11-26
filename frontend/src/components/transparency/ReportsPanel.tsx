import { useCallback, useEffect, useState } from 'react'
import { Download, EyeOff, Eye, Loader2, Upload } from 'lucide-react'
import { createPortalReport, fetchPortalReports, fetchPortfolio, fileToDataUrl, portalReportFileUrl, setPortalReportPublished, type PortalReportRow } from '../../api'
import type { PortfolioEntry } from '../../types'
import { useToast } from '../ToastContext'
import { errorMessage } from '../../lib/errors'
import { cardStyle, fmtDay, fmtSize, inputStyle } from '../../lib/transparency'

/** Informes que se publican en el portal: se suben, se publican y se retiran (no se borran) */
export default function ReportsPanel({ canManage, refreshKey, onChanged }: { canManage: boolean; refreshKey: number; onChanged: () => void }) {
  const { addToast } = useToast()
  const [rows, setRows] = useState<PortalReportRow[] | null>(null)
  const [projects, setProjects] = useState<PortfolioEntry[]>([])
  const [title, setTitle] = useState('')
  const [date, setDate] = useState(new Date().toISOString().slice(0, 10))
  const [projectId, setProjectId] = useState('')
  const [file, setFile] = useState<File | null>(null)
  const [busy, setBusy] = useState(false)

  const load = useCallback(() => {
    fetchPortalReports().then(setRows).catch((e) => { setRows([]); addToast(errorMessage(e), 'error') })
  }, [addToast])
  useEffect(load, [load, refreshKey])
  useEffect(() => { fetchPortfolio().then((p) => setProjects(p.filter((x) => x.entry_type === 'project'))).catch(() => setProjects([])) }, [])

  const upload = async () => {
    if (!file) return
    setBusy(true)
    try {
      await createPortalReport({ title, date, project_id: projectId || null, file: { name: file.name, data: await fileToDataUrl(file) }, publish: true })
      setTitle(''); setFile(null)
      addToast('Informe subido y publicado', 'success')
      load(); onChanged()
    } catch (e) {
      addToast(errorMessage(e), 'error')
    } finally {
      setBusy(false)
    }
  }

  const toggle = async (r: PortalReportRow) => {
    try {
      await setPortalReportPublished(r.id, !r.published)
      addToast(r.published ? 'Informe retirado del portal' : 'Informe publicado', 'success')
      load(); onChanged()
    } catch (e) {
      addToast(errorMessage(e), 'error')
    }
  }

  return (
    <div className="space-y-4">
      {canManage && (
        <div className="rounded-xl p-4 space-y-3" style={cardStyle}>
          <h2 className="text-sm font-semibold" style={{ color: 'var(--text-primary)' }}>Subir y publicar un informe</h2>
          <div className="grid gap-3 sm:grid-cols-2">
            <label className="text-xs sm:col-span-2" style={{ color: 'var(--text-secondary)' }}>Titulo
              <input value={title} maxLength={200} onChange={(e) => setTitle(e.target.value)} className="block w-full mt-1 px-2 py-1.5 rounded-lg text-sm outline-none" style={inputStyle} />
            </label>
            <label className="text-xs" style={{ color: 'var(--text-secondary)' }}>Fecha
              <input type="date" value={date} onChange={(e) => setDate(e.target.value)} className="block w-full mt-1 px-2 py-1.5 rounded-lg text-sm outline-none" style={inputStyle} />
            </label>
            <label className="text-xs" style={{ color: 'var(--text-secondary)' }}>Proyecto (opcional)
              <select value={projectId} onChange={(e) => setProjectId(e.target.value)} className="block w-full mt-1 px-2 py-1.5 rounded-lg text-sm outline-none" style={inputStyle}>
                <option value="">Ninguno</option>
                {projects.map((p) => <option key={p.id} value={p.id}>{p.title}</option>)}
              </select>
            </label>
            <label className="text-xs sm:col-span-2" style={{ color: 'var(--text-secondary)' }}>Archivo (PDF, imagen, XLSX o DOCX, hasta 15 MB)
              <input type="file" accept=".pdf,.png,.jpg,.jpeg,.xlsx,.docx" onChange={(e) => setFile(e.target.files?.[0] ?? null)} className="block w-full mt-1 text-sm" style={{ color: 'var(--text-primary)' }} />
            </label>
          </div>
          <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>Al publicarlo queda disponible para cualquiera con el enlace del portal, y la huella SHA-256 queda en la bitacora.</p>
          <button onClick={upload} disabled={busy || !title.trim() || !file} className="inline-flex items-center gap-2 px-3 py-2 rounded-lg text-sm font-medium disabled:opacity-50" style={{ backgroundColor: 'var(--accent)', color: '#fff' }}>
            {busy ? <Loader2 size={14} className="animate-spin" /> : <Upload size={14} />} Subir y publicar
          </button>
        </div>
      )}
      <div className="rounded-xl overflow-hidden" style={cardStyle}>
        {rows == null && <div className="p-4"><Loader2 className="animate-spin" size={18} style={{ color: 'var(--accent)' }} /></div>}
        {rows?.length === 0 && <p className="p-4 text-sm" style={{ color: 'var(--text-secondary)' }}>Todavia no hay informes.</p>}
        <ul className="divide-y" style={{ borderColor: 'var(--card-border)' }}>
          {rows?.map((r) => (
            <li key={r.id} className="px-4 py-3 flex items-center gap-3">
              <div className="flex-1 min-w-0">
                <p className="text-sm font-medium" style={{ color: 'var(--text-primary)' }}>{r.title}</p>
                <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>{fmtDay(r.date)}{r.project ? ` · ${r.project}` : ''} · {r.file_name} · {fmtSize(r.size)} · subido por {r.created_by}</p>
              </div>
              <span className="text-[11px] px-2 py-0.5 rounded-full" style={{ color: r.published ? 'var(--success)' : 'var(--text-secondary)', border: '1px solid currentColor' }}>{r.published ? 'Publicado' : 'Retirado'}</span>
              <a href={portalReportFileUrl(r.id)} target="_blank" rel="noreferrer" className="p-2" style={{ color: 'var(--accent)' }} title="Descargar"><Download size={16} /></a>
              {canManage && (
                <button onClick={() => toggle(r)} className="p-2" style={{ color: 'var(--text-secondary)' }} title={r.published ? 'Retirar del portal' : 'Publicar'}>
                  {r.published ? <EyeOff size={16} /> : <Eye size={16} />}
                </button>
              )}
            </li>
          ))}
        </ul>
      </div>
    </div>
  )
}

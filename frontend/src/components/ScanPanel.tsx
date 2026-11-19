import { useCallback, useEffect, useState } from 'react'
import { ScanLine, Loader2, RefreshCw, Trash2, Download, FolderDown, X, AlertTriangle } from 'lucide-react'
import {
  fetchScanDevices, fetchScanOptions, scanPage, deleteScanPage, deleteScanSession,
  scanPageUrl, scanDownloadUrl, saveScan,
} from '../api'
import type { ScanDevicesResponse, ScanOptions, ScanSession } from '../types'
import { errorMessage } from '../lib/errors'
import { useToast } from './ToastContext'

const card = { backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }
const input = { backgroundColor: 'var(--bg-tertiary)', color: 'var(--text-primary)', border: '1px solid var(--border)' }
const FOLDER_KEY = 'labnas.scan.folder'

function readFolder(): string {
  try { return localStorage.getItem(FOLDER_KEY) ?? '' } catch { return '' }
}
function rememberFolder(f: string) {
  try { localStorage.setItem(FOLDER_KEY, f) } catch { /* sin almacenamiento local */ }
}

/** Escanear desde la web con el escaner conectado al servidor (SANE) */
export default function ScanPanel() {
  const { addToast } = useToast()
  const [devices, setDevices] = useState<ScanDevicesResponse | null>(null)
  const [device, setDevice] = useState('')
  const [options, setOptions] = useState<ScanOptions | null>(null)
  const [mode, setMode] = useState('')
  const [resolution, setResolution] = useState<number | ''>('')
  const [source, setSource] = useState('')
  const [session, setSession] = useState<ScanSession | null>(null)
  const [scanning, setScanning] = useState(false)
  const [searching, setSearching] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [name, setName] = useState('')
  const [format, setFormat] = useState<'pdf' | 'jpg'>('pdf')
  const [folder, setFolder] = useState(readFolder)
  const [saving, setSaving] = useState(false)

  const loadDevices = useCallback((refresh: boolean) => {
    setSearching(true)
    fetchScanDevices(refresh)
      .then((d) => {
        setDevices(d)
        setDevice((cur) => (d.devices.some((x) => x.id === cur) ? cur : d.devices[0]?.id ?? ''))
      })
      .catch((e) => setError(errorMessage(e)))
      .finally(() => setSearching(false))
  }, [])

  useEffect(() => { loadDevices(false) }, [loadDevices])

  useEffect(() => {
    if (!device) { setOptions(null); return }
    let alive = true
    fetchScanOptions(device)
      .then((o) => {
        if (!alive) return
        setOptions(o)
        setMode(o.default_mode ?? '')
        setSource(o.default_source ?? '')
        // 300 ppp por defecto si existe (buena calidad para documentos); si no, la del escaner
        setResolution(o.resolutions.includes(300) ? 300 : (o.default_resolution ?? ''))
      })
      .catch((e) => { if (alive) setError(errorMessage(e)) })
    return () => { alive = false }
  }, [device])

  async function scan() {
    setScanning(true)
    setError(null)
    try {
      const s = await scanPage({
        session: session?.id,
        device,
        mode: mode || undefined,
        resolution: resolution === '' ? undefined : resolution,
        source: source || undefined,
      })
      setSession(s)
    } catch (e) {
      setError(errorMessage(e))
    } finally {
      setScanning(false)
    }
  }

  async function removePage(i: number) {
    if (!session) return
    try { setSession(await deleteScanPage(session.id, i)) } catch (e) { setError(errorMessage(e)) }
  }

  async function discard() {
    if (!session) return
    await deleteScanSession(session.id).catch(() => {})
    setSession(null)
    setName('')
  }

  async function save() {
    if (!session || !folder.trim()) return
    setSaving(true)
    setError(null)
    try {
      const saved = await saveScan(session.id, folder.trim(), name, format)
      rememberFolder(folder.trim())
      addToast(`Guardado: ${saved.map((p) => p.split('/').pop()).join(', ')}`, 'success')
    } catch (e) {
      setError(errorMessage(e))
    } finally {
      setSaving(false)
    }
  }

  const pages = session?.pages ?? []
  const noScanner = devices && (!devices.available || devices.devices.length === 0)

  return (
    <div className="rounded-xl p-5 space-y-4" style={card}>
      <div className="flex items-center justify-between gap-3">
        <div className="flex items-center gap-2">
          <ScanLine size={18} style={{ color: 'var(--accent)' }} />
          <span className="font-semibold text-sm" style={{ color: 'var(--text-primary)' }}>Escanear</span>
        </div>
        <button
          onClick={() => loadDevices(true)}
          disabled={searching}
          className="p-2 rounded-lg hover:opacity-80"
          style={{ color: 'var(--text-secondary)' }}
          title="Buscar escaneres de nuevo"
        >
          <RefreshCw size={16} className={searching ? 'animate-spin' : ''} />
        </button>
      </div>

      {error && (
        <div className="flex items-start justify-between gap-2 text-sm" style={{ color: 'var(--danger)' }}>
          <span>{error}</span>
          <button onClick={() => setError(null)}><X size={14} /></button>
        </div>
      )}

      {!devices ? (
        <p className="text-sm flex items-center gap-2" style={{ color: 'var(--text-secondary)' }}>
          <Loader2 size={14} className="animate-spin" /> Buscando escaneres...
        </p>
      ) : noScanner ? (
        <p className="text-sm flex items-start gap-2" style={{ color: 'var(--text-secondary)' }}>
          <AlertTriangle size={15} className="shrink-0 mt-0.5" style={{ color: 'var(--warning)' }} />
          <span>
            {devices.error ?? 'No se encontro ningun escaner conectado al servidor.'} Revisa que este encendido; la
            instalacion del driver esta en <code>docs/ESCANER.md</code>.
          </span>
        </p>
      ) : (
        <>
          <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-3">
            <label className="text-xs space-y-1" style={{ color: 'var(--text-secondary)' }}>
              <span>Escaner</span>
              <select value={device} onChange={(e) => setDevice(e.target.value)} className="w-full rounded-lg px-2 py-1.5 text-sm" style={input}>
                {devices.devices.map((d) => <option key={d.id} value={d.id}>{d.name || d.id}</option>)}
              </select>
            </label>
            {options && options.modes.length > 0 && (
              <label className="text-xs space-y-1" style={{ color: 'var(--text-secondary)' }}>
                <span>Modo</span>
                <select value={mode} onChange={(e) => setMode(e.target.value)} className="w-full rounded-lg px-2 py-1.5 text-sm" style={input}>
                  {options.modes.map((m) => <option key={m} value={m}>{m}</option>)}
                </select>
              </label>
            )}
            {options && options.resolutions.length > 0 && (
              <label className="text-xs space-y-1" style={{ color: 'var(--text-secondary)' }}>
                <span>Resolucion</span>
                <select
                  value={resolution}
                  onChange={(e) => setResolution(Number(e.target.value))}
                  className="w-full rounded-lg px-2 py-1.5 text-sm"
                  style={input}
                >
                  {options.resolutions.map((r) => <option key={r} value={r}>{r} ppp</option>)}
                </select>
              </label>
            )}
            {options && options.sources.length > 1 && (
              <label className="text-xs space-y-1" style={{ color: 'var(--text-secondary)' }}>
                <span>Origen</span>
                <select value={source} onChange={(e) => setSource(e.target.value)} className="w-full rounded-lg px-2 py-1.5 text-sm" style={input}>
                  {options.sources.map((s) => <option key={s} value={s}>{s}</option>)}
                </select>
              </label>
            )}
          </div>

          <button
            onClick={scan}
            disabled={scanning || !device}
            className="flex items-center gap-2 px-4 py-2 rounded-lg text-sm font-medium disabled:opacity-60"
            style={{ color: 'var(--accent)', border: '1px solid var(--accent-alpha)', backgroundColor: 'var(--accent-alpha)' }}
          >
            {scanning ? <Loader2 size={16} className="animate-spin" /> : <ScanLine size={16} />}
            {scanning ? 'Escaneando... (puede tardar un minuto)' : pages.length > 0 ? 'Escanear otra pagina' : 'Escanear pagina'}
          </button>
        </>
      )}

      {session && pages.length > 0 && (
        <div className="space-y-3">
          <div className="flex flex-wrap gap-3">
            {pages.map((p, i) => (
              <div key={`${session.id}-${i}-${p.size}`} className="relative rounded-lg overflow-hidden" style={{ border: '1px solid var(--border)' }}>
                <a href={scanPageUrl(session.id, i)} target="_blank" rel="noreferrer">
                  <img src={scanPageUrl(session.id, i)} alt={`Pagina ${i + 1}`} className="h-40 w-auto object-contain bg-white" loading="lazy" />
                </a>
                <div className="flex items-center justify-between px-2 py-1 text-xs" style={{ backgroundColor: 'var(--bg-tertiary)', color: 'var(--text-secondary)' }}>
                  <span>{i + 1} · {p.dpi} ppp</span>
                  <button onClick={() => removePage(i)} title="Quitar pagina" style={{ color: 'var(--danger)' }}><Trash2 size={13} /></button>
                </div>
              </div>
            ))}
          </div>

          <div className="grid grid-cols-1 md:grid-cols-3 gap-3">
            <label className="text-xs space-y-1" style={{ color: 'var(--text-secondary)' }}>
              <span>Nombre del archivo</span>
              <input value={name} onChange={(e) => setName(e.target.value)} placeholder="escaneo-fecha" className="w-full rounded-lg px-2 py-1.5 text-sm" style={input} />
            </label>
            <label className="text-xs space-y-1" style={{ color: 'var(--text-secondary)' }}>
              <span>Formato</span>
              <select value={format} onChange={(e) => setFormat(e.target.value as 'pdf' | 'jpg')} className="w-full rounded-lg px-2 py-1.5 text-sm" style={input}>
                <option value="pdf">PDF (todas las paginas)</option>
                <option value="jpg">JPG (una imagen por pagina)</option>
              </select>
            </label>
            <label className="text-xs space-y-1" style={{ color: 'var(--text-secondary)' }}>
              <span>Carpeta del NAS</span>
              <input value={folder} onChange={(e) => setFolder(e.target.value)} placeholder="/home/lab/Escaneos" className="w-full rounded-lg px-2 py-1.5 text-sm font-mono" style={input} />
            </label>
          </div>

          <div className="flex flex-wrap gap-2">
            <a
              href={scanDownloadUrl(session.id, format, name)}
              className="flex items-center gap-2 px-3 py-2 rounded-lg text-sm"
              style={{ color: 'var(--accent)', border: '1px solid var(--border)' }}
            >
              <Download size={15} /> Descargar
            </a>
            <button
              onClick={save}
              disabled={saving || !folder.trim()}
              className="flex items-center gap-2 px-3 py-2 rounded-lg text-sm disabled:opacity-60"
              style={{ color: 'var(--accent)', border: '1px solid var(--border)' }}
            >
              {saving ? <Loader2 size={15} className="animate-spin" /> : <FolderDown size={15} />} Guardar en la carpeta
            </button>
            <button
              onClick={discard}
              className="flex items-center gap-2 px-3 py-2 rounded-lg text-sm"
              style={{ color: 'var(--danger)', border: '1px solid var(--border)' }}
            >
              <X size={15} /> Descartar y empezar otro
            </button>
          </div>
        </div>
      )}
    </div>
  )
}

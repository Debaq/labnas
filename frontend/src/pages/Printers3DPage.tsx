import { useEffect, useState, useRef, useCallback } from 'react'
import {
  Box,
  Plus,
  Trash2,
  Upload,
  Loader2,
  Thermometer,
  Search,
  Wifi,
  WifiOff,
  X,
} from 'lucide-react'
import {
  fetchPrinters3D,
  addPrinter3D,
  deletePrinter3D,
  fetchPrinter3DStatus,
  uploadGcode,
  detectPrinters3D,
} from '../api'
import type {
  Printer3DConfig,
  Printer3DStatus,
  AddPrinter3DRequest,
  DetectPrintersResult,
} from '../types'

function formatTime(seconds: number | null | undefined): string {
  if (!seconds) return '--'
  const h = Math.floor(seconds / 3600)
  const m = Math.floor((seconds % 3600) / 60)
  return h > 0 ? `${h}h ${m}m` : `${m}m`
}

export default function Printers3DPage() {
  const [printers, setPrinters] = useState<Printer3DConfig[]>([])
  const [statuses, setStatuses] = useState<Record<string, Printer3DStatus>>({})
  const [loading, setLoading] = useState(true)
  const [showAddModal, setShowAddModal] = useState(false)
  const [detecting, setDetecting] = useState(false)
  const [detected, setDetected] = useState<DetectPrintersResult[]>([])
  const [uploading, setUploading] = useState<string | null>(null)
  const [dragOver, setDragOver] = useState<string | null>(null)
  const fileInputRefs = useRef<Record<string, HTMLInputElement | null>>({})

  // Form state
  const [formName, setFormName] = useState('')
  const [formIp, setFormIp] = useState('')
  const [formPort, setFormPort] = useState(5000)
  const [formType, setFormType] = useState<'OctoPrint' | 'Moonraker'>('OctoPrint')
  const [formApiKey, setFormApiKey] = useState('')

  const loadPrinters = useCallback(async () => {
    try {
      const data = await fetchPrinters3D()
      setPrinters(data)
    } catch {
      setPrinters([])
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => {
    loadPrinters()
  }, [loadPrinters])

  // Polling statuses
  useEffect(() => {
    if (printers.length === 0) return

    const fetchAllStatuses = async () => {
      const results: Record<string, Printer3DStatus> = {}
      await Promise.allSettled(
        printers.map(async (p) => {
          try {
            const status = await fetchPrinter3DStatus(p.id)
            results[p.id] = status
          } catch { /* skip */ }
        })
      )
      setStatuses((prev) => ({ ...prev, ...results }))
    }

    fetchAllStatuses()
    const interval = setInterval(fetchAllStatuses, 8000)
    return () => clearInterval(interval)
  }, [printers])

  async function handleAdd() {
    if (!formName.trim() || !formIp.trim()) return
    const req: AddPrinter3DRequest = {
      name: formName,
      ip: formIp,
      port: formPort,
      printer_type: formType,
      api_key: formApiKey || null,
    }
    try {
      await addPrinter3D(req)
      setShowAddModal(false)
      resetForm()
      await loadPrinters()
    } catch (err) {
      console.error('Error agregando impresora:', err)
    }
  }

  async function handleDelete(id: string) {
    if (!confirm('Eliminar esta impresora?')) return
    try {
      await deletePrinter3D(id)
      await loadPrinters()
    } catch (err) {
      console.error('Error eliminando impresora:', err)
    }
  }

  async function handleDetect() {
    setDetecting(true)
    try {
      const results = await detectPrinters3D()
      setDetected(results)
    } catch (err) {
      console.error('Error detectando:', err)
    } finally {
      setDetecting(false)
    }
  }

  function fillFromDetected(d: DetectPrintersResult) {
    setFormIp(d.ip)
    setFormPort(d.port)
    setFormType(d.printer_type)
    setFormName(d.name || `${d.printer_type} @ ${d.ip}`)
    setDetected([])
    setShowAddModal(true)
  }

  async function handleUpload(printerId: string, file: File) {
    setUploading(printerId)
    try {
      await uploadGcode(printerId, file)
    } catch (err) {
      console.error('Error subiendo gcode:', err)
    } finally {
      setUploading(null)
    }
  }

  function handleDrop(e: React.DragEvent, printerId: string) {
    e.preventDefault()
    setDragOver(null)
    const file = e.dataTransfer.files[0]
    if (file) handleUpload(printerId, file)
  }

  function resetForm() {
    setFormName('')
    setFormIp('')
    setFormPort(5000)
    setFormType('OctoPrint')
    setFormApiKey('')
  }

  return (
    <div className="space-y-6">
      {/* Top bar */}
      <div className="flex items-center justify-between flex-wrap gap-4">
        <div className="flex items-center gap-3">
          <span className="text-sm" style={{ color: 'var(--text-secondary)' }}>
            {printers.length} impresora{printers.length !== 1 ? 's' : ''} configurada{printers.length !== 1 ? 's' : ''}
          </span>
        </div>
        <div className="flex items-center gap-3">
          <button
            onClick={handleDetect}
            disabled={detecting}
            className="flex items-center gap-2 px-4 py-2 rounded-lg text-sm font-medium transition-all duration-200 hover:opacity-90"
            style={{
              backgroundColor: 'var(--card-bg)',
              color: 'var(--text-primary)',
              border: '1px solid var(--border)',
            }}
          >
            {detecting ? <Loader2 size={16} className="animate-spin" /> : <Search size={16} />}
            {detecting ? 'Detectando...' : 'Auto-detectar'}
          </button>
          <button
            onClick={() => { resetForm(); setShowAddModal(true) }}
            className="flex items-center gap-2 px-4 py-2 rounded-lg text-sm font-medium transition-all duration-200 hover:opacity-90"
            style={{ backgroundColor: 'var(--accent)', color: '#ffffff' }}
          >
            <Plus size={16} />
            Agregar Impresora
          </button>
        </div>
      </div>

      {/* Detected printers */}
      {detected.length > 0 && (
        <div
          className="rounded-xl p-4 space-y-2"
          style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--accent)' }}
        >
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-semibold" style={{ color: 'var(--accent)' }}>
              Impresoras detectadas en la red
            </h3>
            <button onClick={() => setDetected([])} style={{ color: 'var(--text-secondary)' }}>
              <X size={16} />
            </button>
          </div>
          {detected.map((d, i) => (
            <div key={i} className="flex items-center justify-between py-2" style={{ borderTop: '1px solid var(--border)' }}>
              <div className="text-sm" style={{ color: 'var(--text-primary)' }}>
                <span className="font-mono">{d.ip}:{d.port}</span>
                <span className="ml-2 px-2 py-0.5 rounded text-xs" style={{ backgroundColor: 'var(--accent-alpha)', color: 'var(--accent)' }}>
                  {d.printer_type}
                </span>
                {d.name && <span className="ml-2" style={{ color: 'var(--text-secondary)' }}>{d.name}</span>}
              </div>
              <button
                onClick={() => fillFromDetected(d)}
                className="px-3 py-1 rounded text-xs font-medium"
                style={{ backgroundColor: 'var(--accent)', color: '#ffffff' }}
              >
                Agregar
              </button>
            </div>
          ))}
        </div>
      )}

      {/* Printers Grid */}
      {loading ? (
        <div className="flex items-center justify-center py-16">
          <Loader2 size={32} className="animate-spin" style={{ color: 'var(--accent)' }} />
        </div>
      ) : printers.length === 0 ? (
        <div className="text-center py-16">
          <Box size={48} className="mx-auto mb-4" style={{ color: 'var(--text-secondary)' }} />
          <p className="text-sm" style={{ color: 'var(--text-secondary)' }}>
            No hay impresoras 3D configuradas. Pulsa "Agregar" o "Auto-detectar".
          </p>
        </div>
      ) : (
        <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-6">
          {printers.map((printer) => {
            const status = statuses[printer.id]
            const isOnline = status?.online ?? false
            const temps = status?.temperatures
            const job = status?.current_job
            const isUploading = uploading === printer.id
            const isDraggedOver = dragOver === printer.id

            return (
              <div
                key={printer.id}
                className="rounded-xl p-6 transition-all duration-200 hover:shadow-lg"
                style={{
                  backgroundColor: 'var(--card-bg)',
                  border: isDraggedOver
                    ? '2px dashed var(--accent)'
                    : '1px solid var(--card-border)',
                }}
                onDragOver={(e) => { e.preventDefault(); setDragOver(printer.id) }}
                onDragLeave={() => setDragOver(null)}
                onDrop={(e) => handleDrop(e, printer.id)}
              >
                {/* Header */}
                <div className="flex items-center justify-between mb-4">
                  <div>
                    <h3 className="text-base font-semibold" style={{ color: 'var(--text-primary)' }}>
                      {printer.name}
                    </h3>
                    <p className="text-xs font-mono mt-0.5" style={{ color: 'var(--text-secondary)' }}>
                      {printer.ip}:{printer.port}
                    </p>
                  </div>
                  <div className="flex items-center gap-2">
                    <span
                      className="inline-flex items-center gap-1.5 px-2.5 py-0.5 rounded-full text-xs font-medium"
                      style={{
                        backgroundColor: isOnline ? 'var(--success-alpha)' : 'var(--danger-alpha)',
                        color: isOnline ? 'var(--success)' : 'var(--danger)',
                      }}
                    >
                      {isOnline ? <Wifi size={12} /> : <WifiOff size={12} />}
                      {isOnline ? 'Online' : 'Offline'}
                    </span>
                    <span className="text-xs px-2 py-0.5 rounded" style={{ backgroundColor: 'var(--bg-tertiary)', color: 'var(--text-secondary)' }}>
                      {printer.printer_type}
                    </span>
                  </div>
                </div>

                {/* Temperatures */}
                {temps && (
                  <div className="grid grid-cols-2 gap-3 mb-4">
                    <div className="rounded-lg p-3" style={{ backgroundColor: 'var(--bg-tertiary)' }}>
                      <div className="flex items-center gap-1.5 mb-1">
                        <Thermometer size={14} style={{ color: 'var(--danger)' }} />
                        <span className="text-xs" style={{ color: 'var(--text-secondary)' }}>Hotend</span>
                      </div>
                      <span className="text-sm font-bold" style={{ color: 'var(--text-primary)' }}>
                        {temps.hotend_actual.toFixed(0)}°C
                      </span>
                      <span className="text-xs ml-1" style={{ color: 'var(--text-secondary)' }}>
                        / {temps.hotend_target.toFixed(0)}°C
                      </span>
                    </div>
                    <div className="rounded-lg p-3" style={{ backgroundColor: 'var(--bg-tertiary)' }}>
                      <div className="flex items-center gap-1.5 mb-1">
                        <Thermometer size={14} style={{ color: 'var(--warning)' }} />
                        <span className="text-xs" style={{ color: 'var(--text-secondary)' }}>Cama</span>
                      </div>
                      <span className="text-sm font-bold" style={{ color: 'var(--text-primary)' }}>
                        {temps.bed_actual.toFixed(0)}°C
                      </span>
                      <span className="text-xs ml-1" style={{ color: 'var(--text-secondary)' }}>
                        / {temps.bed_target.toFixed(0)}°C
                      </span>
                    </div>
                  </div>
                )}

                {/* Current Job */}
                {job && (
                  <div className="mb-4">
                    <div className="flex items-center justify-between text-xs mb-1">
                      <span className="font-medium truncate" style={{ color: 'var(--text-primary)' }}>
                        {job.file_name}
                      </span>
                      <span style={{ color: 'var(--accent)' }}>{job.progress.toFixed(1)}%</span>
                    </div>
                    <div className="w-full h-2 rounded-full" style={{ backgroundColor: 'var(--bg-tertiary)' }}>
                      <div
                        className="h-full rounded-full transition-all duration-500"
                        style={{
                          width: `${Math.min(job.progress, 100)}%`,
                          backgroundColor: 'var(--accent)',
                        }}
                      />
                    </div>
                    <div className="flex justify-between text-xs mt-1" style={{ color: 'var(--text-secondary)' }}>
                      <span>{job.state}</span>
                      <span>
                        {formatTime(job.time_elapsed)} / {formatTime(job.time_remaining ? (job.time_elapsed || 0) + job.time_remaining : null)}
                      </span>
                    </div>
                  </div>
                )}

                {/* Actions */}
                <div className="flex items-center gap-2 pt-3" style={{ borderTop: '1px solid var(--border)' }}>
                  <button
                    onClick={() => fileInputRefs.current[printer.id]?.click()}
                    disabled={isUploading}
                    className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium transition-all duration-200 hover:opacity-90"
                    style={{ backgroundColor: 'var(--accent)', color: '#ffffff' }}
                  >
                    {isUploading ? <Loader2 size={14} className="animate-spin" /> : <Upload size={14} />}
                    {isUploading ? 'Subiendo...' : 'Subir .gcode'}
                  </button>
                  <input
                    ref={(el) => { fileInputRefs.current[printer.id] = el }}
                    type="file"
                    accept=".gcode,.gco,.g"
                    className="hidden"
                    onChange={(e) => {
                      const file = e.target.files?.[0]
                      if (file) handleUpload(printer.id, file)
                      e.target.value = ''
                    }}
                  />
                  <div className="flex-1" />
                  <button
                    onClick={() => handleDelete(printer.id)}
                    className="p-1.5 rounded-lg transition-all duration-200 hover:opacity-80"
                    style={{ color: 'var(--danger)' }}
                    title="Eliminar"
                  >
                    <Trash2 size={16} />
                  </button>
                </div>

                {/* Drag zone hint */}
                {isDraggedOver && (
                  <div className="mt-3 text-center py-3 rounded-lg" style={{ backgroundColor: 'var(--accent-alpha)' }}>
                    <span className="text-xs font-medium" style={{ color: 'var(--accent)' }}>
                      Soltar archivo .gcode aqui
                    </span>
                  </div>
                )}
              </div>
            )
          })}
        </div>
      )}

      {/* Add Modal */}
      {showAddModal && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
          <div
            className="rounded-xl p-6 w-full max-w-md mx-4"
            style={{ backgroundColor: 'var(--bg-secondary)', border: '1px solid var(--border)' }}
          >
            <div className="flex items-center justify-between mb-6">
              <h3 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>
                Agregar Impresora 3D
              </h3>
              <button onClick={() => setShowAddModal(false)} style={{ color: 'var(--text-secondary)' }}>
                <X size={20} />
              </button>
            </div>

            <div className="space-y-4">
              <div>
                <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Nombre</label>
                <input
                  type="text"
                  value={formName}
                  onChange={(e) => setFormName(e.target.value)}
                  placeholder="Mi Impresora"
                  className="w-full px-3 py-2 rounded-lg text-sm outline-none"
                  style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
                />
              </div>
              <div className="grid grid-cols-2 gap-3">
                <div>
                  <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>IP</label>
                  <input
                    type="text"
                    value={formIp}
                    onChange={(e) => setFormIp(e.target.value)}
                    placeholder="192.168.1.100"
                    className="w-full px-3 py-2 rounded-lg text-sm outline-none font-mono"
                    style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
                  />
                </div>
                <div>
                  <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Puerto</label>
                  <input
                    type="number"
                    value={formPort}
                    onChange={(e) => setFormPort(parseInt(e.target.value) || 5000)}
                    className="w-full px-3 py-2 rounded-lg text-sm outline-none font-mono"
                    style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
                  />
                </div>
              </div>
              <div>
                <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Tipo</label>
                <select
                  value={formType}
                  onChange={(e) => {
                    const t = e.target.value as 'OctoPrint' | 'Moonraker'
                    setFormType(t)
                    setFormPort(t === 'Moonraker' ? 7125 : 5000)
                  }}
                  className="w-full px-3 py-2 rounded-lg text-sm outline-none cursor-pointer"
                  style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
                >
                  <option value="OctoPrint">OctoPrint</option>
                  <option value="Moonraker">Moonraker</option>
                </select>
              </div>
              <div>
                <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>API Key (opcional)</label>
                <input
                  type="text"
                  value={formApiKey}
                  onChange={(e) => setFormApiKey(e.target.value)}
                  placeholder="Solo para OctoPrint"
                  className="w-full px-3 py-2 rounded-lg text-sm outline-none font-mono"
                  style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
                />
              </div>
            </div>

            <div className="flex items-center justify-end gap-3 mt-6">
              <button
                onClick={() => setShowAddModal(false)}
                className="px-4 py-2 rounded-lg text-sm font-medium"
                style={{ color: 'var(--text-secondary)', border: '1px solid var(--border)' }}
              >
                Cancelar
              </button>
              <button
                onClick={handleAdd}
                className="px-4 py-2 rounded-lg text-sm font-medium"
                style={{ backgroundColor: 'var(--accent)', color: '#ffffff' }}
              >
                Agregar
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  )
}

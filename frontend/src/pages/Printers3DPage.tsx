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
  Play,
  Pause,
  Square,
  RotateCcw,
  Home,
  Send,
  Camera,
  RefreshCw,
  FileText,
  Printer,
  ChevronDown,
  ChevronUp,
  Move,
  Flame,
  ArrowUp,
  ArrowDown,
  ArrowLeft,
  ArrowRight,
} from 'lucide-react'
import {
  fetchPrinters3D,
  addPrinter3D,
  deletePrinter3D,
  fetchPrinter3DStatus,
  uploadGcode,
  detectPrinters3D,
  controlPrint3D,
  preheat3D,
  homeAxes3D,
  jog3D,
  sendGcode3D,
  fetchPrinterFiles,
  printFile3D,
  deletePrinterFile,
  cameraSnapshotUrl,
} from '../api'
import type {
  Printer3DConfig,
  Printer3DStatus,
  AddPrinter3DRequest,
  DetectPrintersResult,
  PrinterFileInfo,
} from '../types'

function formatTime(seconds: number | null | undefined): string {
  if (!seconds) return '--'
  const h = Math.floor(seconds / 3600)
  const m = Math.floor((seconds % 3600) / 60)
  return h > 0 ? `${h}h ${m}m` : `${m}m`
}

function formatFileSize(bytes: number | null | undefined): string {
  if (!bytes) return '--'
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
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
  const [expandedPrinter, setExpandedPrinter] = useState<string | null>(null)
  const [printerFiles, setPrinterFiles] = useState<Record<string, PrinterFileInfo[]>>({})
  const [loadingFiles, setLoadingFiles] = useState<string | null>(null)
  const [jogDistance, setJogDistance] = useState(10)
  const [gcodeInput, setGcodeInput] = useState<Record<string, string>>({})
  const [preheatHotend, setPreheatHotend] = useState(200)
  const [preheatBed, setPreheatBed] = useState(60)
  const [cameraKey, setCameraKey] = useState(0)
  const [actionFeedback, setActionFeedback] = useState<Record<string, string>>({})
  const fileInputRefs = useRef<Record<string, HTMLInputElement | null>>({})

  // Form state
  const [formName, setFormName] = useState('')
  const [formIp, setFormIp] = useState('')
  const [formPort, setFormPort] = useState(5000)
  const [formType, setFormType] = useState<'OctoPrint' | 'Moonraker'>('OctoPrint')
  const [formApiKey, setFormApiKey] = useState('')
  const [formCameraUrl, setFormCameraUrl] = useState('')

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

  // Polling de estados cada 5 segundos
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
    const interval = setInterval(fetchAllStatuses, 5000)
    return () => clearInterval(interval)
  }, [printers])

  // Feedback temporal
  function showFeedback(printerId: string, message: string) {
    setActionFeedback(prev => ({ ...prev, [printerId]: message }))
    setTimeout(() => {
      setActionFeedback(prev => {
        const copy = { ...prev }
        delete copy[printerId]
        return copy
      })
    }, 3000)
  }

  async function handleAdd() {
    if (!formName.trim() || !formIp.trim()) return
    const req: AddPrinter3DRequest = {
      name: formName,
      ip: formIp,
      port: formPort,
      printer_type: formType,
      api_key: formApiKey || null,
      camera_url: formCameraUrl || null,
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
      showFeedback(printerId, `'${file.name}' subido`)
      // Refrescar archivos si esta expandido
      if (expandedPrinter === printerId) {
        loadFiles(printerId)
      }
    } catch (err) {
      console.error('Error subiendo gcode:', err)
      showFeedback(printerId, 'Error al subir archivo')
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

  async function handleControl(printerId: string, command: 'start' | 'pause' | 'resume' | 'cancel') {
    try {
      await controlPrint3D(printerId, command)
      const labels = { start: 'Iniciada', pause: 'Pausada', resume: 'Reanudada', cancel: 'Cancelada' }
      showFeedback(printerId, `Impresion ${labels[command]}`)
    } catch (err) {
      console.error('Error control:', err)
      showFeedback(printerId, 'Error al controlar impresion')
    }
  }

  async function handlePreheat(printerId: string) {
    try {
      await preheat3D(printerId, preheatHotend, preheatBed)
      showFeedback(printerId, `Precalentando: ${preheatHotend}°C / ${preheatBed}°C`)
    } catch (err) {
      console.error('Error preheat:', err)
      showFeedback(printerId, 'Error al precalentar')
    }
  }

  async function handleHome(printerId: string) {
    try {
      await homeAxes3D(printerId)
      showFeedback(printerId, 'Home enviado')
    } catch (err) {
      console.error('Error home:', err)
      showFeedback(printerId, 'Error al hacer home')
    }
  }

  async function handleJog(printerId: string, x: number, y: number, z: number) {
    try {
      await jog3D(printerId, x, y, z)
    } catch (err) {
      console.error('Error jog:', err)
      showFeedback(printerId, 'Error al mover')
    }
  }

  async function handleSendGcode(printerId: string) {
    const cmd = gcodeInput[printerId]?.trim()
    if (!cmd) return
    try {
      await sendGcode3D(printerId, cmd)
      showFeedback(printerId, `Enviado: ${cmd}`)
      setGcodeInput(prev => ({ ...prev, [printerId]: '' }))
    } catch (err) {
      console.error('Error gcode:', err)
      showFeedback(printerId, 'Error al enviar G-code')
    }
  }

  async function loadFiles(printerId: string) {
    setLoadingFiles(printerId)
    try {
      const files = await fetchPrinterFiles(printerId)
      setPrinterFiles(prev => ({ ...prev, [printerId]: files }))
    } catch (err) {
      console.error('Error files:', err)
    } finally {
      setLoadingFiles(null)
    }
  }

  async function handlePrintFile(printerId: string, filename: string) {
    try {
      await printFile3D(printerId, filename)
      showFeedback(printerId, `Imprimiendo '${filename}'`)
    } catch (err) {
      console.error('Error print file:', err)
      showFeedback(printerId, 'Error al imprimir archivo')
    }
  }

  async function handleDeleteFile(printerId: string, filename: string) {
    if (!confirm(`Eliminar '${filename}'?`)) return
    try {
      await deletePrinterFile(printerId, filename)
      loadFiles(printerId)
    } catch (err) {
      console.error('Error delete file:', err)
    }
  }

  function toggleExpand(printerId: string) {
    if (expandedPrinter === printerId) {
      setExpandedPrinter(null)
    } else {
      setExpandedPrinter(printerId)
      loadFiles(printerId)
    }
  }

  function resetForm() {
    setFormName('')
    setFormIp('')
    setFormPort(5000)
    setFormType('OctoPrint')
    setFormApiKey('')
    setFormCameraUrl('')
  }

  // Barra de progreso de temperatura
  function TempBar({ actual, target, color }: { actual: number; target: number; color: string }) {
    const pct = target > 0 ? Math.min((actual / target) * 100, 100) : 0
    return (
      <div className="w-full h-1.5 rounded-full mt-1" style={{ backgroundColor: 'var(--bg-tertiary)' }}>
        <div
          className="h-full rounded-full transition-all duration-500"
          style={{ width: `${pct}%`, backgroundColor: color }}
        />
      </div>
    )
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

      {/* Printers */}
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
        <div className="space-y-4">
          {printers.map((printer) => {
            const status = statuses[printer.id]
            const isOnline = status?.online ?? false
            const temps = status?.temperatures
            const job = status?.current_job
            const isUploading = uploading === printer.id
            const isDraggedOver = dragOver === printer.id
            const isExpanded = expandedPrinter === printer.id
            const files = printerFiles[printer.id] || []
            const feedback = actionFeedback[printer.id]
            const isPrinting = job && (job.state.toLowerCase().includes('printing') || job.state.toLowerCase() === 'printing')
            const isPaused = job && job.state.toLowerCase().includes('paus')

            return (
              <div
                key={printer.id}
                className="rounded-xl transition-all duration-200"
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
                <div className="p-6 pb-0">
                  <div className="flex items-center justify-between mb-4">
                    <div className="flex items-center gap-3">
                      <Printer size={20} style={{ color: isOnline ? 'var(--accent)' : 'var(--text-secondary)' }} />
                      <div>
                        <h3 className="text-base font-semibold" style={{ color: 'var(--text-primary)' }}>
                          {printer.name}
                        </h3>
                        <p className="text-xs font-mono mt-0.5" style={{ color: 'var(--text-secondary)' }}>
                          {printer.ip}:{printer.port}
                        </p>
                      </div>
                    </div>
                    <div className="flex items-center gap-2">
                      {feedback && (
                        <span className="text-xs px-2 py-1 rounded-lg animate-pulse" style={{ backgroundColor: 'var(--accent-alpha)', color: 'var(--accent)' }}>
                          {feedback}
                        </span>
                      )}
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
                  {isOnline && temps && (
                    <div className="grid grid-cols-2 gap-3 mb-4">
                      <div className="rounded-lg p-3" style={{ backgroundColor: 'var(--bg-tertiary)' }}>
                        <div className="flex items-center justify-between mb-1">
                          <div className="flex items-center gap-1.5">
                            <Thermometer size={14} style={{ color: 'var(--danger)' }} />
                            <span className="text-xs" style={{ color: 'var(--text-secondary)' }}>Hotend</span>
                          </div>
                          <span className="text-sm font-bold" style={{ color: 'var(--text-primary)' }}>
                            {temps.hotend_actual.toFixed(0)}°C
                            <span className="text-xs font-normal ml-1" style={{ color: 'var(--text-secondary)' }}>
                              / {temps.hotend_target.toFixed(0)}°C
                            </span>
                          </span>
                        </div>
                        <TempBar actual={temps.hotend_actual} target={temps.hotend_target} color="var(--danger)" />
                      </div>
                      <div className="rounded-lg p-3" style={{ backgroundColor: 'var(--bg-tertiary)' }}>
                        <div className="flex items-center justify-between mb-1">
                          <div className="flex items-center gap-1.5">
                            <Thermometer size={14} style={{ color: 'var(--warning)' }} />
                            <span className="text-xs" style={{ color: 'var(--text-secondary)' }}>Cama</span>
                          </div>
                          <span className="text-sm font-bold" style={{ color: 'var(--text-primary)' }}>
                            {temps.bed_actual.toFixed(0)}°C
                            <span className="text-xs font-normal ml-1" style={{ color: 'var(--text-secondary)' }}>
                              / {temps.bed_target.toFixed(0)}°C
                            </span>
                          </span>
                        </div>
                        <TempBar actual={temps.bed_actual} target={temps.bed_target} color="var(--warning)" />
                      </div>
                    </div>
                  )}

                  {/* Current Job */}
                  {isOnline && job && job.file_name && (
                    <div className="mb-4 rounded-lg p-3" style={{ backgroundColor: 'var(--bg-tertiary)' }}>
                      <div className="flex items-center justify-between text-xs mb-2">
                        <span className="font-medium truncate flex items-center gap-1.5" style={{ color: 'var(--text-primary)' }}>
                          <FileText size={12} />
                          {job.file_name}
                        </span>
                        <span className="font-bold" style={{ color: 'var(--accent)' }}>{job.progress.toFixed(1)}%</span>
                      </div>
                      <div className="w-full h-2.5 rounded-full" style={{ backgroundColor: 'var(--card-bg)' }}>
                        <div
                          className="h-full rounded-full transition-all duration-500"
                          style={{
                            width: `${Math.min(job.progress, 100)}%`,
                            backgroundColor: isPaused ? 'var(--warning)' : 'var(--accent)',
                          }}
                        />
                      </div>
                      <div className="flex justify-between text-xs mt-2" style={{ color: 'var(--text-secondary)' }}>
                        <span className="px-1.5 py-0.5 rounded text-xs font-medium" style={{
                          backgroundColor: isPrinting ? 'var(--success-alpha)' : isPaused ? 'var(--warning-alpha)' : 'var(--bg-tertiary)',
                          color: isPrinting ? 'var(--success)' : isPaused ? 'var(--warning)' : 'var(--text-secondary)',
                        }}>
                          {job.state}
                        </span>
                        <span>
                          {formatTime(job.time_elapsed)} / {formatTime(job.time_remaining ? (job.time_elapsed || 0) + job.time_remaining : null)}
                        </span>
                      </div>

                      {/* Botones de control de impresión */}
                      <div className="flex items-center gap-2 mt-3 pt-3" style={{ borderTop: '1px solid var(--border)' }}>
                        {!isPrinting && !isPaused && (
                          <button
                            onClick={() => handleControl(printer.id, 'start')}
                            className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium hover:opacity-90"
                            style={{ backgroundColor: 'var(--success)', color: '#ffffff' }}
                            title="Iniciar"
                          >
                            <Play size={12} /> Iniciar
                          </button>
                        )}
                        {isPrinting && (
                          <button
                            onClick={() => handleControl(printer.id, 'pause')}
                            className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium hover:opacity-90"
                            style={{ backgroundColor: 'var(--warning)', color: '#ffffff' }}
                            title="Pausar"
                          >
                            <Pause size={12} /> Pausar
                          </button>
                        )}
                        {isPaused && (
                          <button
                            onClick={() => handleControl(printer.id, 'resume')}
                            className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium hover:opacity-90"
                            style={{ backgroundColor: 'var(--success)', color: '#ffffff' }}
                            title="Reanudar"
                          >
                            <Play size={12} /> Reanudar
                          </button>
                        )}
                        {(isPrinting || isPaused) && (
                          <button
                            onClick={() => {
                              if (confirm('Cancelar la impresion actual?')) {
                                handleControl(printer.id, 'cancel')
                              }
                            }}
                            className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium hover:opacity-90"
                            style={{ backgroundColor: 'var(--danger)', color: '#ffffff' }}
                            title="Cancelar"
                          >
                            <Square size={12} /> Cancelar
                          </button>
                        )}
                      </div>
                    </div>
                  )}
                </div>

                {/* Actions bar */}
                <div className="px-6 py-3 flex items-center gap-2 flex-wrap" style={{ borderTop: '1px solid var(--border)' }}>
                  <button
                    onClick={() => fileInputRefs.current[printer.id]?.click()}
                    disabled={isUploading || !isOnline}
                    className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium transition-all duration-200 hover:opacity-90 disabled:opacity-50"
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

                  {isOnline && (
                    <button
                      onClick={() => toggleExpand(printer.id)}
                      className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium transition-all duration-200 hover:opacity-90"
                      style={{
                        backgroundColor: isExpanded ? 'var(--accent-alpha)' : 'var(--bg-tertiary)',
                        color: isExpanded ? 'var(--accent)' : 'var(--text-primary)',
                        border: isExpanded ? '1px solid var(--accent)' : '1px solid var(--border)',
                      }}
                    >
                      {isExpanded ? <ChevronUp size={14} /> : <ChevronDown size={14} />}
                      Controles
                    </button>
                  )}

                  {isOnline && printer.camera_url && (
                    <button
                      onClick={() => setCameraKey(k => k + 1)}
                      className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium transition-all duration-200 hover:opacity-90"
                      style={{ backgroundColor: 'var(--bg-tertiary)', color: 'var(--text-primary)', border: '1px solid var(--border)' }}
                    >
                      <Camera size={14} /> Camara
                    </button>
                  )}

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
                  <div className="mx-6 mb-3 text-center py-3 rounded-lg" style={{ backgroundColor: 'var(--accent-alpha)' }}>
                    <span className="text-xs font-medium" style={{ color: 'var(--accent)' }}>
                      Soltar archivo .gcode aqui
                    </span>
                  </div>
                )}

                {/* Camera snapshot */}
                {isOnline && printer.camera_url && (
                  <div className="px-6 pb-4">
                    <div className="rounded-lg overflow-hidden" style={{ backgroundColor: 'var(--bg-tertiary)' }}>
                      <div className="flex items-center justify-between px-3 py-2">
                        <span className="text-xs font-medium" style={{ color: 'var(--text-secondary)' }}>
                          <Camera size={12} className="inline mr-1" />Camara
                        </span>
                        <button
                          onClick={() => setCameraKey(k => k + 1)}
                          className="text-xs flex items-center gap-1 hover:opacity-80"
                          style={{ color: 'var(--accent)' }}
                        >
                          <RefreshCw size={12} /> Refrescar
                        </button>
                      </div>
                      <img
                        key={cameraKey}
                        src={`${cameraSnapshotUrl(printer.id)}?t=${cameraKey}`}
                        alt="Camera"
                        className="w-full"
                        style={{ maxHeight: 300, objectFit: 'contain' }}
                        onError={(e) => {
                          (e.target as HTMLImageElement).style.display = 'none'
                        }}
                      />
                    </div>
                  </div>
                )}

                {/* Expanded controls */}
                {isExpanded && isOnline && (
                  <div className="px-6 pb-6 space-y-4" style={{ borderTop: '1px solid var(--border)' }}>
                    {/* Precalentar */}
                    <div className="pt-4">
                      <h4 className="text-xs font-semibold mb-2 flex items-center gap-1.5" style={{ color: 'var(--text-secondary)' }}>
                        <Flame size={14} /> Precalentar
                      </h4>
                      <div className="flex items-center gap-2 flex-wrap">
                        <div className="flex items-center gap-1">
                          <span className="text-xs" style={{ color: 'var(--text-secondary)' }}>Hotend:</span>
                          <input
                            type="number"
                            value={preheatHotend}
                            onChange={(e) => setPreheatHotend(parseInt(e.target.value) || 0)}
                            className="w-16 px-2 py-1 rounded text-xs text-center outline-none"
                            style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
                          />
                          <span className="text-xs" style={{ color: 'var(--text-secondary)' }}>°C</span>
                        </div>
                        <div className="flex items-center gap-1">
                          <span className="text-xs" style={{ color: 'var(--text-secondary)' }}>Cama:</span>
                          <input
                            type="number"
                            value={preheatBed}
                            onChange={(e) => setPreheatBed(parseInt(e.target.value) || 0)}
                            className="w-16 px-2 py-1 rounded text-xs text-center outline-none"
                            style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
                          />
                          <span className="text-xs" style={{ color: 'var(--text-secondary)' }}>°C</span>
                        </div>
                        <button
                          onClick={() => handlePreheat(printer.id)}
                          className="flex items-center gap-1 px-3 py-1.5 rounded-lg text-xs font-medium hover:opacity-90"
                          style={{ backgroundColor: 'var(--danger)', color: '#ffffff' }}
                        >
                          <Flame size={12} /> Calentar
                        </button>
                        <button
                          onClick={async () => {
                            try {
                              await preheat3D(printer.id, 0, 0)
                              showFeedback(printer.id, 'Enfriando...')
                            } catch {}
                          }}
                          className="px-3 py-1.5 rounded-lg text-xs font-medium hover:opacity-90"
                          style={{ backgroundColor: 'var(--bg-tertiary)', color: 'var(--text-primary)', border: '1px solid var(--border)' }}
                        >
                          Enfriar
                        </button>
                      </div>
                    </div>

                    {/* Jog Controls */}
                    <div>
                      <h4 className="text-xs font-semibold mb-2 flex items-center gap-1.5" style={{ color: 'var(--text-secondary)' }}>
                        <Move size={14} /> Control de ejes
                      </h4>
                      <div className="flex items-start gap-4 flex-wrap">
                        {/* XY pad */}
                        <div className="grid grid-cols-3 gap-1" style={{ width: 120 }}>
                          <div />
                          <button
                            onClick={() => handleJog(printer.id, 0, jogDistance, 0)}
                            className="p-2 rounded-lg hover:opacity-80 flex items-center justify-center"
                            style={{ backgroundColor: 'var(--bg-tertiary)', border: '1px solid var(--border)' }}
                            title={`Y+${jogDistance}`}
                          >
                            <ArrowUp size={16} style={{ color: 'var(--text-primary)' }} />
                          </button>
                          <div />
                          <button
                            onClick={() => handleJog(printer.id, -jogDistance, 0, 0)}
                            className="p-2 rounded-lg hover:opacity-80 flex items-center justify-center"
                            style={{ backgroundColor: 'var(--bg-tertiary)', border: '1px solid var(--border)' }}
                            title={`X-${jogDistance}`}
                          >
                            <ArrowLeft size={16} style={{ color: 'var(--text-primary)' }} />
                          </button>
                          <button
                            onClick={() => handleHome(printer.id)}
                            className="p-2 rounded-lg hover:opacity-80 flex items-center justify-center"
                            style={{ backgroundColor: 'var(--accent-alpha)', border: '1px solid var(--accent)' }}
                            title="Home"
                          >
                            <Home size={16} style={{ color: 'var(--accent)' }} />
                          </button>
                          <button
                            onClick={() => handleJog(printer.id, jogDistance, 0, 0)}
                            className="p-2 rounded-lg hover:opacity-80 flex items-center justify-center"
                            style={{ backgroundColor: 'var(--bg-tertiary)', border: '1px solid var(--border)' }}
                            title={`X+${jogDistance}`}
                          >
                            <ArrowRight size={16} style={{ color: 'var(--text-primary)' }} />
                          </button>
                          <div />
                          <button
                            onClick={() => handleJog(printer.id, 0, -jogDistance, 0)}
                            className="p-2 rounded-lg hover:opacity-80 flex items-center justify-center"
                            style={{ backgroundColor: 'var(--bg-tertiary)', border: '1px solid var(--border)' }}
                            title={`Y-${jogDistance}`}
                          >
                            <ArrowDown size={16} style={{ color: 'var(--text-primary)' }} />
                          </button>
                          <div />
                        </div>

                        {/* Z controls */}
                        <div className="flex flex-col gap-1 items-center">
                          <span className="text-xs mb-1" style={{ color: 'var(--text-secondary)' }}>Z</span>
                          <button
                            onClick={() => handleJog(printer.id, 0, 0, jogDistance)}
                            className="p-2 rounded-lg hover:opacity-80 flex items-center justify-center"
                            style={{ backgroundColor: 'var(--bg-tertiary)', border: '1px solid var(--border)', width: 38 }}
                            title={`Z+${jogDistance}`}
                          >
                            <ArrowUp size={16} style={{ color: 'var(--text-primary)' }} />
                          </button>
                          <button
                            onClick={() => handleJog(printer.id, 0, 0, -jogDistance)}
                            className="p-2 rounded-lg hover:opacity-80 flex items-center justify-center"
                            style={{ backgroundColor: 'var(--bg-tertiary)', border: '1px solid var(--border)', width: 38 }}
                            title={`Z-${jogDistance}`}
                          >
                            <ArrowDown size={16} style={{ color: 'var(--text-primary)' }} />
                          </button>
                        </div>

                        {/* Distance selector */}
                        <div className="flex flex-col gap-1">
                          <span className="text-xs mb-1" style={{ color: 'var(--text-secondary)' }}>Distancia</span>
                          {[0.1, 1, 10, 100].map(d => (
                            <button
                              key={d}
                              onClick={() => setJogDistance(d)}
                              className="px-3 py-1 rounded text-xs font-mono font-medium hover:opacity-90"
                              style={{
                                backgroundColor: jogDistance === d ? 'var(--accent)' : 'var(--bg-tertiary)',
                                color: jogDistance === d ? '#ffffff' : 'var(--text-primary)',
                                border: `1px solid ${jogDistance === d ? 'var(--accent)' : 'var(--border)'}`,
                              }}
                            >
                              {d}mm
                            </button>
                          ))}
                        </div>
                      </div>
                    </div>

                    {/* G-code manual */}
                    <div>
                      <h4 className="text-xs font-semibold mb-2 flex items-center gap-1.5" style={{ color: 'var(--text-secondary)' }}>
                        <Send size={14} /> G-code manual
                      </h4>
                      <div className="flex items-center gap-2">
                        <input
                          type="text"
                          value={gcodeInput[printer.id] || ''}
                          onChange={(e) => setGcodeInput(prev => ({ ...prev, [printer.id]: e.target.value }))}
                          onKeyDown={(e) => {
                            if (e.key === 'Enter') handleSendGcode(printer.id)
                          }}
                          placeholder="G28, M104 S200, etc."
                          className="flex-1 px-3 py-1.5 rounded-lg text-xs outline-none font-mono"
                          style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
                        />
                        <button
                          onClick={() => handleSendGcode(printer.id)}
                          className="flex items-center gap-1 px-3 py-1.5 rounded-lg text-xs font-medium hover:opacity-90"
                          style={{ backgroundColor: 'var(--accent)', color: '#ffffff' }}
                        >
                          <Send size={12} /> Enviar
                        </button>
                      </div>
                    </div>

                    {/* Archivos en la impresora */}
                    <div>
                      <div className="flex items-center justify-between mb-2">
                        <h4 className="text-xs font-semibold flex items-center gap-1.5" style={{ color: 'var(--text-secondary)' }}>
                          <FileText size={14} /> Archivos en la impresora
                        </h4>
                        <button
                          onClick={() => loadFiles(printer.id)}
                          disabled={loadingFiles === printer.id}
                          className="text-xs flex items-center gap-1 hover:opacity-80"
                          style={{ color: 'var(--accent)' }}
                        >
                          {loadingFiles === printer.id ? <Loader2 size={12} className="animate-spin" /> : <RefreshCw size={12} />}
                          Refrescar
                        </button>
                      </div>

                      {files.length === 0 ? (
                        <p className="text-xs py-2" style={{ color: 'var(--text-secondary)' }}>
                          {loadingFiles === printer.id ? 'Cargando...' : 'No hay archivos'}
                        </p>
                      ) : (
                        <div className="rounded-lg overflow-hidden" style={{ border: '1px solid var(--border)' }}>
                          {files.map((file, i) => (
                            <div
                              key={file.name}
                              className="flex items-center justify-between px-3 py-2 text-xs"
                              style={{
                                borderTop: i > 0 ? '1px solid var(--border)' : undefined,
                                backgroundColor: i % 2 === 0 ? 'var(--bg-tertiary)' : 'transparent',
                              }}
                            >
                              <div className="flex items-center gap-2 min-w-0 flex-1">
                                <FileText size={12} style={{ color: 'var(--text-secondary)', flexShrink: 0 }} />
                                <span className="truncate font-mono" style={{ color: 'var(--text-primary)' }}>{file.name}</span>
                                <span style={{ color: 'var(--text-secondary)', flexShrink: 0 }}>{formatFileSize(file.size)}</span>
                              </div>
                              <div className="flex items-center gap-1 ml-2">
                                <button
                                  onClick={() => handlePrintFile(printer.id, file.name)}
                                  className="p-1 rounded hover:opacity-80"
                                  style={{ color: 'var(--success)' }}
                                  title="Imprimir"
                                >
                                  <Play size={14} />
                                </button>
                                <button
                                  onClick={() => handleDeleteFile(printer.id, file.name)}
                                  className="p-1 rounded hover:opacity-80"
                                  style={{ color: 'var(--danger)' }}
                                  title="Eliminar"
                                >
                                  <Trash2 size={14} />
                                </button>
                              </div>
                            </div>
                          ))}
                        </div>
                      )}
                    </div>
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
            className="rounded-xl p-6 w-full max-w-md mx-4 max-h-[90vh] overflow-y-auto"
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
              <div>
                <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>URL de camara (opcional)</label>
                <input
                  type="text"
                  value={formCameraUrl}
                  onChange={(e) => setFormCameraUrl(e.target.value)}
                  placeholder="http://192.168.1.100/webcam/?action=snapshot"
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

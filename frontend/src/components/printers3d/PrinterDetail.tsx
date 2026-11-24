import { useState, useRef } from 'react'
import { Upload, Loader2, Wifi, WifiOff, Printer, ChevronDown, ChevronUp, Camera, RefreshCw, ExternalLink, Pencil, Trash2 } from 'lucide-react'
import { uploadGcode, fetchPrinterFiles, controlPrint3D, cameraSnapshotUrl } from '../../api'
import type { Printer3DConfig, Printer3DStatus, PrinterFileInfo } from '../../types'
import { printerWebUrl } from './utils'
import PrinterControls from './PrinterControls'
import { TemperaturesPanel, JobPanel } from './PrinterStatusPanels'

interface Props {
  printer: Printer3DConfig
  status: Printer3DStatus | undefined
  onEdit: (printer: Printer3DConfig) => void
  onDelete: (id: string) => void
}

export default function PrinterDetail({ printer, status, onEdit, onDelete }: Props) {
  const [uploading, setUploading] = useState<string | null>(null)
  const [dragOver, setDragOver] = useState<string | null>(null)
  const [expandedPrinter, setExpandedPrinter] = useState<string | null>(null)
  const [printerFiles, setPrinterFiles] = useState<Record<string, PrinterFileInfo[]>>({})
  const [loadingFiles, setLoadingFiles] = useState<string | null>(null)
  const [cameraKey, setCameraKey] = useState(0)
  const [actionFeedback, setActionFeedback] = useState<Record<string, string>>({})
  const fileInputRefs = useRef<Record<string, HTMLInputElement | null>>({})

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
  function toggleExpand(printerId: string) {
    if (expandedPrinter === printerId) {
      setExpandedPrinter(null)
    } else {
      setExpandedPrinter(printerId)
      loadFiles(printerId)
    }
  }

  const isOnline = status?.online ?? false
  const temps = status?.temperatures
  const job = status?.current_job
  const isUploading = uploading === printer.id
  const isDraggedOver = dragOver === printer.id
  const isExpanded = expandedPrinter === printer.id
  const files = printerFiles[printer.id] || []
  const feedback = actionFeedback[printer.id]

  return (
    <div
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
              <p className="text-xs font-mono mt-0.5 flex items-center gap-1.5" style={{ color: 'var(--text-secondary)' }}>
                {printer.ip}:{printer.port}
                {printerWebUrl(printer) && (
                  <a
                    href={printerWebUrl(printer)!}
                    target="_blank"
                    rel="noopener noreferrer"
                    className="inline-flex items-center hover:opacity-70 transition-opacity"
                    style={{ color: 'var(--accent)' }}
                    title="Abrir gestor web"
                  >
                    <ExternalLink size={11} />
                  </a>
                )}
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

          {isOnline && temps && <TemperaturesPanel temps={temps} />}

          {isOnline && job && job.file_name && (
            <JobPanel job={job} onControl={(command) => handleControl(printer.id, command)} />
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
          onClick={() => onEdit(printer)}
          className="p-1.5 rounded-lg transition-all duration-200 hover:opacity-80"
          style={{ color: 'var(--text-secondary)' }}
          title="Editar"
        >
          <Pencil size={14} />
        </button>
        <button
          onClick={() => onDelete(printer.id)}
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


        {isExpanded && isOnline && (
          <PrinterControls
            printerId={printer.id}
            files={files}
            loadingFiles={loadingFiles === printer.id}
            onLoadFiles={() => loadFiles(printer.id)}
            showFeedback={showFeedback}
          />
        )}
      </div>
  )
}

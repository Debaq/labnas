import { useEffect, useState, useRef } from 'react'
import {
  Printer,
  Upload,
  Loader2,
  X,
  FileText,
  Trash2,
  RefreshCw,
  Settings2,
  Play,
  Pause,
} from 'lucide-react'
import {
  fetchCupsPrinters,
  fetchPrinterOptions,
  fetchPrintJobs,
  printFileUpload,
  cancelPrintJob,
  enablePrinter,
  disablePrinter,
} from '../api'
import type { CupsPrinter, CupsPrintJob, PrinterOption } from '../types'

export default function PrintingPage() {
  const [printers, setPrinters] = useState<CupsPrinter[]>([])
  const [jobs, setJobs] = useState<CupsPrintJob[]>([])
  const [loading, setLoading] = useState(true)
  const [printing, setPrinting] = useState(false)
  const [error, setError] = useState<string | null>(null)

  // Print modal
  const [showModal, setShowModal] = useState(false)
  const [selectedFile, setSelectedFile] = useState<File | null>(null)
  const [selectedPrinter, setSelectedPrinter] = useState('')
  const [copies, setCopies] = useState<number | string>(1)
  const [pages, setPages] = useState('')

  // Dynamic printer options
  const [printerOptions, setPrinterOptions] = useState<PrinterOption[]>([])
  const [optionValues, setOptionValues] = useState<Record<string, string>>({})
  const [optionsLoading, setOptionsLoading] = useState(false)

  // Drag & drop
  const [dragOver, setDragOver] = useState(false)
  const fileInputRef = useRef<HTMLInputElement>(null)

  useEffect(() => {
    loadData()
  }, [])

  async function loadData() {
    setLoading(true)
    setError(null)
    try {
      const [p, j] = await Promise.allSettled([fetchCupsPrinters(), fetchPrintJobs()])
      if (p.status === 'fulfilled') {
        setPrinters(p.value)
        if (p.value.length > 0 && !selectedPrinter) {
          const def = p.value.find((pr) => pr.is_default)
          setSelectedPrinter(def?.name || p.value[0].name)
        }
      } else {
        setError('No se pudo conectar con CUPS. Verifica que este instalado.')
      }
      if (j.status === 'fulfilled') setJobs(j.value)
    } finally {
      setLoading(false)
    }
  }

  async function loadPrinterOptions(printerName: string) {
    setOptionsLoading(true)
    try {
      const opts = await fetchPrinterOptions(printerName)
      setPrinterOptions(opts)
      // Set defaults
      const defaults: Record<string, string> = {}
      for (const opt of opts) {
        defaults[opt.key] = opt.default_value
      }
      setOptionValues(defaults)
    } catch {
      setPrinterOptions([])
      setOptionValues({})
    } finally {
      setOptionsLoading(false)
    }
  }

  function openPrintModal(file: File) {
    setSelectedFile(file)
    setCopies(1)
    setPages('')
    setShowModal(true)
    if (selectedPrinter) {
      loadPrinterOptions(selectedPrinter)
    }
  }

  function handlePrinterChange(name: string) {
    setSelectedPrinter(name)
    if (showModal) {
      loadPrinterOptions(name)
    }
  }

  async function handlePrint() {
    if (!selectedFile || !selectedPrinter) return
    setPrinting(true)
    try {
      // Only send options that differ from defaults
      const changedOptions: Record<string, string> = {}
      for (const opt of printerOptions) {
        const current = optionValues[opt.key]
        if (current && current !== opt.default_value) {
          changedOptions[opt.key] = current
        }
      }

      await printFileUpload(selectedFile, selectedPrinter, {
        copies: Number(copies) || 1,
        pages: pages || undefined,
        options: changedOptions,
      })
      setShowModal(false)
      setSelectedFile(null)
      const j = await fetchPrintJobs()
      setJobs(j)
    } catch (err) {
      console.error('Error imprimiendo:', err)
    } finally {
      setPrinting(false)
    }
  }

  async function handleCancel(id: string) {
    try {
      await cancelPrintJob(id)
      const j = await fetchPrintJobs()
      setJobs(j)
    } catch (err) {
      console.error('Error cancelando:', err)
    }
  }

  const printableExts = ['pdf', 'ps', 'eps', 'txt', 'text', 'log', 'conf', 'sh', 'py', 'rs', 'js', 'ts', 'json', 'xml', 'csv', 'md', 'c', 'cpp', 'h', 'java', 'png', 'jpg', 'jpeg', 'gif', 'tiff', 'tif', 'bmp', 'svg']
  const printableAccept = printableExts.map(e => `.${e}`).join(',')

  function isPrintableFile(name: string): boolean {
    const ext = name.split('.').pop()?.toLowerCase() || ''
    return printableExts.includes(ext)
  }

  function handleDrop(e: React.DragEvent) {
    e.preventDefault()
    setDragOver(false)
    const file = e.dataTransfer.files[0]
    if (!file) return
    if (!isPrintableFile(file.name)) {
      alert('Formato no soportado. Usa PDF, imagenes (PNG/JPG) o texto plano.')
      return
    }
    openPrintModal(file)
  }

  function handleFileSelect(e: React.ChangeEvent<HTMLInputElement>) {
    const file = e.target.files?.[0]
    if (file) openPrintModal(file)
    e.target.value = ''
  }

  const stateColor = (state: string) => {
    if (state === 'idle') return 'var(--success)'
    if (state === 'printing') return 'var(--accent)'
    if (state === 'disabled') return 'var(--danger)'
    return 'var(--text-secondary)'
  }

  const stateLabel = (state: string) => {
    if (state === 'idle') return 'Libre'
    if (state === 'printing') return 'Imprimiendo'
    if (state === 'disabled') return 'Deshabilitada'
    return state
  }

  return (
    <div className="space-y-6">
      {/* Error banner */}
      {error && (
        <div
          className="rounded-xl p-4 text-sm"
          style={{ backgroundColor: 'var(--danger-alpha)', color: 'var(--danger)', border: '1px solid var(--danger)' }}
        >
          {error}
        </div>
      )}

      {/* Printers */}
      <div>
        <div className="flex items-center justify-between mb-4">
          <h2 className="text-base font-semibold" style={{ color: 'var(--text-primary)' }}>
            Impresoras CUPS
          </h2>
          <button
            onClick={loadData}
            className="p-2 rounded-lg transition-all duration-200 hover:opacity-80"
            style={{ color: 'var(--text-secondary)' }}
            title="Refrescar"
          >
            <RefreshCw size={16} />
          </button>
        </div>

        {loading ? (
          <div className="flex items-center justify-center py-8">
            <Loader2 size={24} className="animate-spin" style={{ color: 'var(--accent)' }} />
          </div>
        ) : printers.length === 0 ? (
          <div
            className="rounded-xl p-8 text-center"
            style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}
          >
            <Printer size={40} className="mx-auto mb-3" style={{ color: 'var(--text-secondary)' }} />
            <p className="text-sm" style={{ color: 'var(--text-secondary)' }}>
              No se detectaron impresoras CUPS
            </p>
          </div>
        ) : (
          <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
            {printers.map((p) => (
              <div
                key={p.name}
                className="rounded-xl p-5 transition-all duration-200 hover:shadow-lg"
                style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}
              >
                <div className="flex items-center justify-between mb-2">
                  <div className="flex items-center gap-2">
                    <Printer size={18} style={{ color: 'var(--accent)' }} />
                    <span className="text-sm font-semibold" style={{ color: 'var(--text-primary)' }}>
                      {p.description}
                    </span>
                  </div>
                  {p.is_default && (
                    <span className="text-xs px-2 py-0.5 rounded-full font-medium" style={{ backgroundColor: 'var(--accent-alpha)', color: 'var(--accent)' }}>
                      Default
                    </span>
                  )}
                </div>
                <div className="flex items-center gap-2 mt-2">
                  <span
                    className="w-2 h-2 rounded-full"
                    style={{ backgroundColor: stateColor(p.state) }}
                  />
                  <span className="text-xs" style={{ color: stateColor(p.state) }}>
                    {stateLabel(p.state)}
                  </span>
                  <span className="text-xs font-mono ml-auto" style={{ color: 'var(--text-secondary)' }}>
                    {p.name}
                  </span>
                  {p.state === 'disabled' ? (
                    <button
                      onClick={async (e) => {
                        e.stopPropagation()
                        try {
                          await enablePrinter(p.name)
                          await loadData()
                        } catch {}
                      }}
                      className="p-1 rounded-lg transition-all duration-200 hover:opacity-80"
                      style={{ color: 'var(--success)' }}
                      title="Reanudar impresora"
                    >
                      <Play size={14} />
                    </button>
                  ) : (
                    <button
                      onClick={async (e) => {
                        e.stopPropagation()
                        try {
                          await disablePrinter(p.name)
                          await loadData()
                        } catch {}
                      }}
                      className="p-1 rounded-lg transition-all duration-200 hover:opacity-80"
                      style={{ color: 'var(--text-secondary)' }}
                      title="Pausar impresora"
                    >
                      <Pause size={14} />
                    </button>
                  )}
                </div>
              </div>
            ))}
          </div>
        )}
      </div>

      {/* Drop zone */}
      <div
        className="rounded-xl p-8 text-center cursor-pointer transition-all duration-200"
        style={{
          backgroundColor: dragOver ? 'var(--accent-alpha)' : 'var(--card-bg)',
          border: dragOver ? '2px dashed var(--accent)' : '2px dashed var(--card-border)',
        }}
        onClick={() => fileInputRef.current?.click()}
        onDragOver={(e) => { e.preventDefault(); setDragOver(true) }}
        onDragLeave={() => setDragOver(false)}
        onDrop={handleDrop}
      >
        <Upload size={36} className="mx-auto mb-3" style={{ color: dragOver ? 'var(--accent)' : 'var(--text-secondary)' }} />
        <p className="text-sm font-medium" style={{ color: 'var(--text-primary)' }}>
          Arrastra un archivo aqui o haz clic para seleccionar
        </p>
        <p className="text-xs mt-1" style={{ color: 'var(--text-secondary)' }}>
          PDF, imagenes (PNG, JPG, GIF, BMP, TIFF, SVG), texto plano, PostScript
        </p>
        <input
          ref={fileInputRef}
          type="file"
          accept={printableAccept}
          className="hidden"
          onChange={handleFileSelect}
        />
      </div>

      {/* Print Queue */}
      <div>
        <h2 className="text-base font-semibold mb-4" style={{ color: 'var(--text-primary)' }}>
          Cola de Impresion
        </h2>
        <div
          className="rounded-xl overflow-hidden"
          style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}
        >
          {jobs.length === 0 ? (
            <div className="text-center py-8">
              <FileText size={32} className="mx-auto mb-2" style={{ color: 'var(--text-secondary)' }} />
              <p className="text-sm" style={{ color: 'var(--text-secondary)' }}>Cola vacia</p>
            </div>
          ) : (
            <table className="w-full">
              <thead>
                <tr style={{ borderBottom: '1px solid var(--border)' }}>
                  <th className="text-left px-6 py-3 text-xs font-medium uppercase tracking-wider" style={{ color: 'var(--text-secondary)' }}>ID</th>
                  <th className="text-left px-6 py-3 text-xs font-medium uppercase tracking-wider" style={{ color: 'var(--text-secondary)' }}>Impresora</th>
                  <th className="text-left px-6 py-3 text-xs font-medium uppercase tracking-wider" style={{ color: 'var(--text-secondary)' }}>Titulo</th>
                  <th className="text-left px-6 py-3 text-xs font-medium uppercase tracking-wider" style={{ color: 'var(--text-secondary)' }}>Tamano</th>
                  <th className="text-right px-6 py-3 text-xs font-medium uppercase tracking-wider" style={{ color: 'var(--text-secondary)' }}>Acciones</th>
                </tr>
              </thead>
              <tbody>
                {jobs.map((job) => (
                  <tr key={job.id} style={{ borderBottom: '1px solid var(--border)' }}>
                    <td className="px-6 py-3 text-sm font-mono" style={{ color: 'var(--text-primary)' }}>{job.id}</td>
                    <td className="px-6 py-3 text-sm" style={{ color: 'var(--text-secondary)' }}>{job.printer}</td>
                    <td className="px-6 py-3 text-sm" style={{ color: 'var(--text-primary)' }}>{job.title}</td>
                    <td className="px-6 py-3 text-sm" style={{ color: 'var(--text-secondary)' }}>{job.size || '--'}</td>
                    <td className="px-6 py-3 text-right">
                      <button
                        onClick={() => handleCancel(job.id)}
                        className="p-1.5 rounded-lg transition-all duration-200 hover:opacity-80"
                        style={{ color: 'var(--danger)' }}
                        title="Cancelar"
                      >
                        <Trash2 size={16} />
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </div>
      </div>

      {/* Print Options Modal */}
      {showModal && selectedFile && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
          <div
            className="rounded-xl p-6 w-full max-w-lg mx-4 max-h-[90vh] overflow-y-auto"
            style={{ backgroundColor: 'var(--bg-secondary)', border: '1px solid var(--border)' }}
          >
            <div className="flex items-center justify-between mb-6">
              <h3 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>
                Opciones de Impresion
              </h3>
              <button onClick={() => setShowModal(false)} style={{ color: 'var(--text-secondary)' }}>
                <X size={20} />
              </button>
            </div>

            {/* File info */}
            <div className="rounded-lg p-3 mb-4" style={{ backgroundColor: 'var(--bg-tertiary)' }}>
              <div className="flex items-center gap-2">
                <FileText size={16} style={{ color: 'var(--accent)' }} />
                <span className="text-sm font-medium truncate" style={{ color: 'var(--text-primary)' }}>
                  {selectedFile.name}
                </span>
              </div>
            </div>

            <div className="space-y-4">
              {/* Printer selector */}
              <div>
                <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Impresora</label>
                <select
                  value={selectedPrinter}
                  onChange={(e) => handlePrinterChange(e.target.value)}
                  className="w-full px-3 py-2 rounded-lg text-sm outline-none cursor-pointer"
                  style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
                >
                  {printers.map((p) => (
                    <option key={p.name} value={p.name}>
                      {p.description} {p.is_default ? '(default)' : ''}
                    </option>
                  ))}
                </select>
              </div>

              {/* Copies & Pages */}
              <div className="grid grid-cols-2 gap-3">
                <div>
                  <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Copias</label>
                  <input
                    type="number"
                    min={1}
                    max={100}
                    value={copies}
                    onChange={(e) => setCopies(e.target.value === '' ? '' : parseInt(e.target.value) || '')}
                    onBlur={() => setCopies(prev => { const n = Number(prev); return (!n || n < 1) ? 1 : Math.min(n, 100) })}
                    className="w-full px-3 py-2 rounded-lg text-sm outline-none"
                    style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
                  />
                </div>
                <div>
                  <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Paginas</label>
                  <input
                    type="text"
                    value={pages}
                    onChange={(e) => setPages(e.target.value)}
                    placeholder="Todas"
                    className="w-full px-3 py-2 rounded-lg text-sm outline-none"
                    style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
                  />
                </div>
              </div>

              {/* Dynamic printer options */}
              {optionsLoading ? (
                <div className="flex items-center justify-center py-4">
                  <Loader2 size={18} className="animate-spin" style={{ color: 'var(--accent)' }} />
                  <span className="text-xs ml-2" style={{ color: 'var(--text-secondary)' }}>Cargando opciones...</span>
                </div>
              ) : printerOptions.length > 0 && (
                <div>
                  <div className="flex items-center gap-2 mb-3 pt-2" style={{ borderTop: '1px solid var(--border)' }}>
                    <Settings2 size={14} style={{ color: 'var(--accent)' }} />
                    <span className="text-xs font-medium" style={{ color: 'var(--text-secondary)' }}>
                      Opciones de la impresora
                    </span>
                  </div>
                  <div className="grid grid-cols-2 gap-3">
                    {printerOptions.map((opt) => (
                      <div key={opt.key}>
                        <label className="block text-xs font-medium mb-1 truncate" title={opt.display_name} style={{ color: 'var(--text-secondary)' }}>
                          {opt.display_name}
                        </label>
                        <select
                          value={optionValues[opt.key] || opt.default_value}
                          onChange={(e) => setOptionValues(prev => ({ ...prev, [opt.key]: e.target.value }))}
                          className="w-full px-3 py-2 rounded-lg text-sm outline-none cursor-pointer"
                          style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
                        >
                          {opt.values.map((v) => (
                            <option key={v} value={v}>
                              {v}{v === opt.default_value ? ' *' : ''}
                            </option>
                          ))}
                        </select>
                      </div>
                    ))}
                  </div>
                </div>
              )}
            </div>

            {/* Actions */}
            <div className="flex items-center justify-end gap-3 mt-6">
              <button
                onClick={() => setShowModal(false)}
                className="px-4 py-2 rounded-lg text-sm font-medium"
                style={{ color: 'var(--text-secondary)', border: '1px solid var(--border)' }}
              >
                Cancelar
              </button>
              <button
                onClick={handlePrint}
                disabled={printing}
                className="flex items-center gap-2 px-4 py-2 rounded-lg text-sm font-medium"
                style={{ backgroundColor: 'var(--accent)', color: '#ffffff' }}
              >
                {printing ? <Loader2 size={16} className="animate-spin" /> : <Printer size={16} />}
                {printing ? 'Imprimiendo...' : 'Imprimir'}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  )
}

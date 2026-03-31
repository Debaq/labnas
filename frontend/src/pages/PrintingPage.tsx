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
  BarChart3,
  Save,
  RotateCcw,
  Zap,
  Users,
  User,
  ChevronDown,
  ChevronRight,
  DollarSign,
} from 'lucide-react'
import {
  fetchCupsPrinters,
  fetchPrinterOptions,
  fetchPrintJobs,
  printFileUpload,
  cancelPrintJob,
  enablePrinter,
  disablePrinter,
  fetchPrinterStats,
  setPrinterCosts,
  resetPrinterStats,
  wakePrinter,
  fetchAllUserCosts,
  fetchMyCosts,
} from '../api'
import { useAuth } from '../auth/AuthContext'
import type { CupsPrinter, CupsPrintJob, PrinterOption, PrinterStatsResponse, AllUserCostsResponse } from '../types'

export default function PrintingPage() {
  const { isAdmin } = useAuth()
  const [printers, setPrinters] = useState<CupsPrinter[]>([])
  const [jobs, setJobs] = useState<CupsPrintJob[]>([])
  const [loading, setLoading] = useState(true)
  const [printing, setPrinting] = useState(false)
  const [error, setError] = useState<string | null>(null)

  // User costs
  const [userCosts, setUserCosts] = useState<AllUserCostsResponse | null>(null)
  const [userCostsLoading, setUserCostsLoading] = useState(false)
  const [showUserCosts, setShowUserCosts] = useState(false)
  const [expandedUser, setExpandedUser] = useState<string | null>(null)

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

  // Printer stats/costs
  const [expandedStats, setExpandedStats] = useState<string | null>(null)
  const [printerStats, setPrinterStats] = useState<Record<string, PrinterStatsResponse>>({})
  const [editingCosts, setEditingCosts] = useState<Record<string, { ink_per_page: string; paper_carta: string; paper_oficio: string; paper_special: string }>>({})
  const [savingCosts, setSavingCosts] = useState(false)

  // Standard print options
  const [orientation, setOrientation] = useState('')
  const [pageSet, setPageSet] = useState('')
  const [numberUp, setNumberUp] = useState('1')
  const [collate, setCollate] = useState(true)
  const [outputOrder, setOutputOrder] = useState('')
  const [fitToPage, setFitToPage] = useState(false)

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

  async function loadUserCosts() {
    setUserCostsLoading(true)
    try {
      const data = isAdmin ? await fetchAllUserCosts() : await fetchMyCosts()
      setUserCosts(data)
    } catch {}
    setUserCostsLoading(false)
  }

  async function loadStats(printerName: string) {
    try {
      const stats = await fetchPrinterStats(printerName)
      setPrinterStats(prev => ({ ...prev, [printerName]: stats }))
      setEditingCosts(prev => ({
        ...prev,
        [printerName]: {
          ink_per_page: stats.costs.ink_per_page ? String(stats.costs.ink_per_page) : '',
          paper_carta: stats.costs.paper_carta ? String(stats.costs.paper_carta) : '',
          paper_oficio: stats.costs.paper_oficio ? String(stats.costs.paper_oficio) : '',
          paper_special: stats.costs.paper_special ? String(stats.costs.paper_special) : '',
        },
      }))
    } catch { /* ignore */ }
  }

  async function handleSaveCosts(printerName: string) {
    const c = editingCosts[printerName]
    if (!c) return
    setSavingCosts(true)
    try {
      await setPrinterCosts(printerName, {
        ink_per_page: parseFloat(c.ink_per_page) || 0,
        paper_carta: parseFloat(c.paper_carta) || 0,
        paper_oficio: parseFloat(c.paper_oficio) || 0,
        paper_special: parseFloat(c.paper_special) || 0,
      })
      await loadStats(printerName)
    } catch { /* ignore */ }
    finally { setSavingCosts(false) }
  }

  async function handleResetStats(printerName: string) {
    if (!confirm('Resetear estadisticas de esta impresora?')) return
    try {
      await resetPrinterStats(printerName)
      await loadStats(printerName)
    } catch { /* ignore */ }
  }

  function toggleStats(printerName: string) {
    if (expandedStats === printerName) {
      setExpandedStats(null)
    } else {
      setExpandedStats(printerName)
      if (!printerStats[printerName]) loadStats(printerName)
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
    setOrientation('')
    setPageSet('')
    setNumberUp('1')
    setCollate(true)
    setOutputOrder('')
    setFitToPage(false)
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
      // Standard options
      if (orientation) changedOptions['orientation-requested'] = orientation
      if (pageSet) changedOptions['page-set'] = pageSet
      if (numberUp !== '1') changedOptions['number-up'] = numberUp
      if (Number(copies) > 1 && !collate) changedOptions['collate'] = 'false'
      if (outputOrder) changedOptions['outputorder'] = outputOrder
      if (fitToPage) changedOptions['fit-to-page'] = 'true'

      await printFileUpload(selectedFile, selectedPrinter, {
        copies: Number(copies) || 1,
        pages: pages || undefined,
        options: changedOptions,
      })
      setShowModal(false)
      setSelectedFile(null)
      const j = await fetchPrintJobs()
      setJobs(j)
      // Refresh stats if expanded
      if (expandedStats === selectedPrinter) loadStats(selectedPrinter)
    } catch (err: any) {
      console.error('Error imprimiendo:', err)
      alert('Error al imprimir: ' + (err.message || err))
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

  const standardOptionKeys = ['orientation-requested', 'page-set', 'number-up', 'collate', 'outputorder', 'fit-to-page']
  const filteredPrinterOptions = printerOptions.filter(opt => !standardOptionKeys.includes(opt.key))

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
                  <button
                    onClick={async (e) => {
                      e.stopPropagation()
                      try {
                        await wakePrinter(p.name)
                        await loadData()
                      } catch {}
                    }}
                    className="p-1 rounded-lg transition-all duration-200 hover:opacity-80"
                    style={{ color: 'var(--warning)' }}
                    title="Despertar y habilitar"
                  >
                    <Zap size={14} />
                  </button>
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
                  <button
                    onClick={() => toggleStats(p.name)}
                    className="p-1 rounded-lg transition-all duration-200 hover:opacity-80"
                    style={{ color: expandedStats === p.name ? 'var(--accent)' : 'var(--text-secondary)' }}
                    title="Estadisticas y costos"
                  >
                    <BarChart3 size={14} />
                  </button>
                </div>

                {/* Stats panel */}
                {expandedStats === p.name && (() => {
                  const data = printerStats[p.name]
                  const costs = editingCosts[p.name]
                  if (!data) return (
                    <div className="mt-3 pt-3 flex justify-center" style={{ borderTop: '1px solid var(--border)' }}>
                      <Loader2 size={16} className="animate-spin" style={{ color: 'var(--accent)' }} />
                    </div>
                  )
                  return (
                    <div className="mt-3 pt-3 space-y-3" style={{ borderTop: '1px solid var(--border)' }}>
                      {/* Stats */}
                      <div className="grid grid-cols-3 gap-2 text-center">
                        <div className="rounded-lg p-2" style={{ backgroundColor: 'var(--bg-tertiary)' }}>
                          <div className="text-lg font-bold font-mono" style={{ color: 'var(--text-primary)' }}>{data.stats.total_jobs}</div>
                          <div className="text-[10px]" style={{ color: 'var(--text-secondary)' }}>Trabajos</div>
                        </div>
                        <div className="rounded-lg p-2" style={{ backgroundColor: 'var(--bg-tertiary)' }}>
                          <div className="text-lg font-bold font-mono" style={{ color: 'var(--text-primary)' }}>{data.stats.total_pages}</div>
                          <div className="text-[10px]" style={{ color: 'var(--text-secondary)' }}>Paginas</div>
                        </div>
                        <div className="rounded-lg p-2" style={{ backgroundColor: 'var(--bg-tertiary)' }}>
                          <div className="text-lg font-bold font-mono" style={{ color: data.estimated_cost > 0 ? 'var(--accent)' : 'var(--text-primary)' }}>
                            {data.estimated_cost > 0 ? `$${data.estimated_cost.toFixed(0)}` : '--'}
                          </div>
                          <div className="text-[10px]" style={{ color: 'var(--text-secondary)' }}>Costo est.</div>
                        </div>
                      </div>

                      {/* Pages breakdown */}
                      {data.stats.total_pages > 0 && (
                        <div className="flex gap-3 text-[10px]" style={{ color: 'var(--text-secondary)' }}>
                          {data.stats.pages_carta > 0 && <span>Carta: {data.stats.pages_carta}</span>}
                          {data.stats.pages_oficio > 0 && <span>Oficio: {data.stats.pages_oficio}</span>}
                          {data.stats.pages_special > 0 && <span>Especial: {data.stats.pages_special}</span>}
                        </div>
                      )}

                      {/* Cost config */}
                      {costs && (
                        <div>
                          <div className="text-[10px] font-medium mb-1.5" style={{ color: 'var(--text-secondary)' }}>Costos por pagina</div>
                          <div className="grid grid-cols-2 gap-2">
                            {([
                              ['ink_per_page', 'Tinta/Toner'],
                              ['paper_carta', 'Papel carta'],
                              ['paper_oficio', 'Papel oficio'],
                              ['paper_special', 'Papel especial'],
                            ] as const).map(([key, label]) => (
                              <div key={key} className="relative">
                                <input
                                  type="number"
                                  min={0}
                                  step="any"
                                  value={costs[key]}
                                  onChange={(e) => setEditingCosts(prev => ({
                                    ...prev,
                                    [p.name]: { ...prev[p.name], [key]: e.target.value },
                                  }))}
                                  placeholder={label}
                                  className="w-full px-2 py-1.5 rounded-lg text-xs outline-none pr-6"
                                  style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
                                />
                                <span className="absolute right-2 top-1/2 -translate-y-1/2 text-[10px] pointer-events-none" style={{ color: 'var(--text-secondary)', opacity: 0.5 }}>$</span>
                                <span className="block text-[9px] mt-0.5" style={{ color: 'var(--text-secondary)', opacity: 0.6 }}>{label}</span>
                              </div>
                            ))}
                          </div>
                          <div className="flex items-center gap-2 mt-2">
                            <button
                              onClick={() => handleSaveCosts(p.name)}
                              disabled={savingCosts}
                              className="flex items-center gap-1 text-[10px] px-2 py-1 rounded-lg"
                              style={{ backgroundColor: 'var(--accent)', color: '#fff' }}
                            >
                              {savingCosts ? <Loader2 size={10} className="animate-spin" /> : <Save size={10} />}
                              Guardar
                            </button>
                            {data.stats.total_jobs > 0 && (
                              <button
                                onClick={() => handleResetStats(p.name)}
                                className="flex items-center gap-1 text-[10px] px-2 py-1 rounded-lg"
                                style={{ color: 'var(--text-secondary)', border: '1px solid var(--border)' }}
                              >
                                <RotateCcw size={10} /> Resetear stats
                              </button>
                            )}
                          </div>
                        </div>
                      )}
                    </div>
                  )
                })()}
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

      {/* User Costs */}
      <div>
        <button
          onClick={() => {
            const next = !showUserCosts
            setShowUserCosts(next)
            if (next && !userCosts) loadUserCosts()
          }}
          className="flex items-center gap-2 text-base font-semibold mb-4 hover:opacity-80 transition-opacity"
          style={{ color: 'var(--text-primary)' }}
        >
          {showUserCosts ? <ChevronDown size={18} /> : <ChevronRight size={18} />}
          <DollarSign size={18} style={{ color: 'var(--accent)' }} />
          Costos por Usuario
        </button>

        {showUserCosts && (
          <div
            className="rounded-xl p-4 space-y-4"
            style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}
          >
            {userCostsLoading ? (
              <div className="flex justify-center py-6">
                <Loader2 size={20} className="animate-spin" style={{ color: 'var(--accent)' }} />
              </div>
            ) : !userCosts ? (
              <p className="text-sm text-center py-4" style={{ color: 'var(--text-secondary)' }}>
                No se pudieron cargar los costos
              </p>
            ) : (
              <>
                {/* General totals */}
                <div className="grid grid-cols-3 gap-3 text-center">
                  <div className="rounded-lg p-3" style={{ backgroundColor: 'var(--bg-tertiary)' }}>
                    <div className="text-xl font-bold font-mono" style={{ color: 'var(--text-primary)' }}>
                      {userCosts.general_jobs}
                    </div>
                    <div className="text-[10px]" style={{ color: 'var(--text-secondary)' }}>Trabajos totales</div>
                  </div>
                  <div className="rounded-lg p-3" style={{ backgroundColor: 'var(--bg-tertiary)' }}>
                    <div className="text-xl font-bold font-mono" style={{ color: 'var(--text-primary)' }}>
                      {userCosts.general_pages}
                    </div>
                    <div className="text-[10px]" style={{ color: 'var(--text-secondary)' }}>Paginas totales</div>
                  </div>
                  <div className="rounded-lg p-3" style={{ backgroundColor: 'var(--bg-tertiary)' }}>
                    <div className="text-xl font-bold font-mono" style={{ color: userCosts.general_cost > 0 ? 'var(--accent)' : 'var(--text-primary)' }}>
                      {userCosts.general_cost > 0 ? `$${userCosts.general_cost.toFixed(0)}` : '--'}
                    </div>
                    <div className="text-[10px]" style={{ color: 'var(--text-secondary)' }}>Costo total</div>
                  </div>
                </div>

                {/* Per user */}
                {userCosts.users.length === 0 ? (
                  <p className="text-xs text-center py-3" style={{ color: 'var(--text-secondary)' }}>
                    Aun no hay impresiones registradas por usuario
                  </p>
                ) : (
                  <div className="space-y-2">
                    {userCosts.users.map((u) => (
                      <div
                        key={u.username}
                        className="rounded-lg overflow-hidden"
                        style={{ backgroundColor: 'var(--bg-tertiary)', border: '1px solid var(--border)' }}
                      >
                        <button
                          onClick={() => setExpandedUser(expandedUser === u.username ? null : u.username)}
                          className="w-full flex items-center gap-3 px-4 py-3 text-left hover:opacity-80 transition-opacity"
                        >
                          {isAdmin ? <Users size={14} style={{ color: 'var(--accent)' }} /> : <User size={14} style={{ color: 'var(--accent)' }} />}
                          <span className="text-sm font-medium flex-1" style={{ color: 'var(--text-primary)' }}>
                            {u.username}
                          </span>
                          <span className="text-xs font-mono" style={{ color: 'var(--text-secondary)' }}>
                            {u.total_jobs} trabajos
                          </span>
                          <span className="text-xs font-mono" style={{ color: 'var(--text-secondary)' }}>
                            {u.total_pages} pag
                          </span>
                          <span className="text-sm font-bold font-mono" style={{ color: u.total_cost > 0 ? 'var(--accent)' : 'var(--text-secondary)' }}>
                            {u.total_cost > 0 ? `$${u.total_cost.toFixed(0)}` : '--'}
                          </span>
                          {expandedUser === u.username ? <ChevronDown size={14} style={{ color: 'var(--text-secondary)' }} /> : <ChevronRight size={14} style={{ color: 'var(--text-secondary)' }} />}
                        </button>

                        {expandedUser === u.username && u.printers.length > 0 && (
                          <div className="px-4 pb-3 space-y-2" style={{ borderTop: '1px solid var(--border)' }}>
                            <div className="pt-2 text-[10px] font-medium" style={{ color: 'var(--text-secondary)' }}>
                              Desglose por impresora
                            </div>
                            {u.printers.map((pr) => (
                              <div
                                key={pr.printer}
                                className="flex items-center gap-3 rounded-md px-3 py-2"
                                style={{ backgroundColor: 'var(--bg-primary)' }}
                              >
                                <Printer size={12} style={{ color: 'var(--text-secondary)' }} />
                                <span className="text-xs font-mono flex-1" style={{ color: 'var(--text-primary)' }}>
                                  {pr.printer}
                                </span>
                                <span className="text-[10px]" style={{ color: 'var(--text-secondary)' }}>
                                  {pr.stats.total_jobs}j / {pr.stats.total_pages}p
                                </span>
                                {pr.stats.pages_carta > 0 && (
                                  <span className="text-[10px]" style={{ color: 'var(--text-secondary)' }}>C:{pr.stats.pages_carta}</span>
                                )}
                                {pr.stats.pages_oficio > 0 && (
                                  <span className="text-[10px]" style={{ color: 'var(--text-secondary)' }}>O:{pr.stats.pages_oficio}</span>
                                )}
                                {pr.stats.pages_special > 0 && (
                                  <span className="text-[10px]" style={{ color: 'var(--text-secondary)' }}>E:{pr.stats.pages_special}</span>
                                )}
                                <span className="text-xs font-bold font-mono" style={{ color: pr.estimated_cost > 0 ? 'var(--accent)' : 'var(--text-secondary)' }}>
                                  {pr.estimated_cost > 0 ? `$${pr.estimated_cost.toFixed(0)}` : '--'}
                                </span>
                              </div>
                            ))}
                          </div>
                        )}
                      </div>
                    ))}
                  </div>
                )}

                <div className="flex justify-end">
                  <button
                    onClick={loadUserCosts}
                    className="flex items-center gap-1 text-[10px] px-2 py-1 rounded-lg hover:opacity-80"
                    style={{ color: 'var(--text-secondary)', border: '1px solid var(--border)' }}
                  >
                    <RefreshCw size={10} /> Actualizar
                  </button>
                </div>
              </>
            )}
          </div>
        )}
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

              {/* Standard print options */}
              <div>
                <div className="flex items-center gap-2 mb-3 pt-2" style={{ borderTop: '1px solid var(--border)' }}>
                  <Settings2 size={14} style={{ color: 'var(--accent)' }} />
                  <span className="text-xs font-medium" style={{ color: 'var(--text-secondary)' }}>
                    Opciones de pagina
                  </span>
                </div>
                <div className="grid grid-cols-2 gap-3">
                  <div>
                    <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Orientacion</label>
                    <select
                      value={orientation}
                      onChange={(e) => setOrientation(e.target.value)}
                      className="w-full px-3 py-2 rounded-lg text-sm outline-none cursor-pointer"
                      style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
                    >
                      <option value="">Automatica</option>
                      <option value="3">Vertical (retrato)</option>
                      <option value="4">Horizontal (paisaje)</option>
                    </select>
                  </div>
                  <div>
                    <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Conjunto de paginas</label>
                    <select
                      value={pageSet}
                      onChange={(e) => setPageSet(e.target.value)}
                      className="w-full px-3 py-2 rounded-lg text-sm outline-none cursor-pointer"
                      style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
                    >
                      <option value="">Todas</option>
                      <option value="odd">Solo impares (1, 3, 5...)</option>
                      <option value="even">Solo pares (2, 4, 6...)</option>
                    </select>
                    <span className="block text-[10px] mt-0.5" style={{ color: 'var(--text-secondary)', opacity: 0.7 }}>Para imprimir a doble cara manual</span>
                  </div>
                  <div>
                    <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Paginas por hoja</label>
                    <select
                      value={numberUp}
                      onChange={(e) => setNumberUp(e.target.value)}
                      className="w-full px-3 py-2 rounded-lg text-sm outline-none cursor-pointer"
                      style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
                    >
                      <option value="1">1 (normal)</option>
                      <option value="2">2 en 1</option>
                      <option value="4">4 en 1</option>
                      <option value="6">6 en 1</option>
                      <option value="9">9 en 1</option>
                      <option value="16">16 en 1</option>
                    </select>
                    {numberUp !== '1' && <span className="block text-[10px] mt-0.5" style={{ color: 'var(--text-secondary)', opacity: 0.7 }}>{numberUp} paginas reducidas en cada hoja</span>}
                  </div>
                  <div>
                    <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Orden de salida</label>
                    <select
                      value={outputOrder}
                      onChange={(e) => setOutputOrder(e.target.value)}
                      className="w-full px-3 py-2 rounded-lg text-sm outline-none cursor-pointer"
                      style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
                    >
                      <option value="">Normal (1, 2, 3...)</option>
                      <option value="reverse">Inverso (...3, 2, 1)</option>
                    </select>
                    {outputOrder === 'reverse' && <span className="block text-[10px] mt-0.5" style={{ color: 'var(--text-secondary)', opacity: 0.7 }}>Las hojas salen ordenadas boca arriba</span>}
                  </div>
                </div>
                <div className="flex flex-col gap-2 mt-3">
                  {Number(copies) > 1 && (
                    <label className="flex items-start gap-2 cursor-pointer">
                      <input
                        type="checkbox"
                        checked={collate}
                        onChange={(e) => setCollate(e.target.checked)}
                        className="mt-0.5"
                        style={{ accentColor: 'var(--accent)' }}
                      />
                      <span className="text-xs" style={{ color: 'var(--text-secondary)' }}>
                        Intercalar copias
                        <span className="block text-[10px] mt-0.5" style={{ opacity: 0.7 }}>
                          {collate ? 'Cada copia completa: 1-2-3, 1-2-3' : 'Agrupar por pagina: 1-1, 2-2, 3-3'}
                        </span>
                      </span>
                    </label>
                  )}
                  <label className="flex items-start gap-2 cursor-pointer">
                    <input
                      type="checkbox"
                      checked={fitToPage}
                      onChange={(e) => setFitToPage(e.target.checked)}
                      className="mt-0.5"
                      style={{ accentColor: 'var(--accent)' }}
                    />
                    <span className="text-xs" style={{ color: 'var(--text-secondary)' }}>
                      Ajustar a pagina
                      <span className="block text-[10px] mt-0.5" style={{ opacity: 0.7 }}>Escala el contenido para que entre en la hoja</span>
                    </span>
                  </label>
                </div>
              </div>

              {/* Dynamic printer options */}
              {optionsLoading ? (
                <div className="flex items-center justify-center py-4">
                  <Loader2 size={18} className="animate-spin" style={{ color: 'var(--accent)' }} />
                  <span className="text-xs ml-2" style={{ color: 'var(--text-secondary)' }}>Cargando opciones...</span>
                </div>
              ) : filteredPrinterOptions.length > 0 && (
                <div>
                  <div className="flex items-center gap-2 mb-3 pt-2" style={{ borderTop: '1px solid var(--border)' }}>
                    <Settings2 size={14} style={{ color: 'var(--accent)' }} />
                    <span className="text-xs font-medium" style={{ color: 'var(--text-secondary)' }}>
                      Opciones de la impresora
                    </span>
                  </div>
                  <div className="grid grid-cols-2 gap-3">
                    {filteredPrinterOptions.map((opt) => (
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

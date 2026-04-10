import { useState, useCallback, useEffect, useRef } from 'react'
import {
  X,
  Loader2,
  Printer,
  FileText,
  RotateCcw,
  Check,
  AlertTriangle,
  BookOpen,
  ArrowDown,
  ArrowUp,
  Layers,
  Settings2,
  ChevronDown,
  ChevronRight,
} from 'lucide-react'
import { duplexPrepare, duplexPrintStep, duplexCleanup, fetchPrinterOptions } from '../api'
import type { CupsPrinter, DuplexPrepareResponse, PrinterOption } from '../types'

interface DuplexWizardProps {
  printers: CupsPrinter[]
  defaultPrinter: string
  onClose: () => void
  onComplete: () => void
}

type WizardStep = 'upload' | 'configure' | 'printing-odd' | 'flip' | 'printing-even' | 'batch-done' | 'complete'

export default function DuplexWizard({ printers, defaultPrinter, onClose, onComplete }: DuplexWizardProps) {
  const [step, setStep] = useState<WizardStep>('upload')
  const [prepareData, setPrepareData] = useState<DuplexPrepareResponse | null>(null)
  const [preparing, setPreparing] = useState(false)
  const [error, setError] = useState<string | null>(null)

  // Configuracion
  const [printer, setPrinter] = useState(defaultPrinter)
  const [totalCopies, setTotalCopies] = useState(1)
  const [batchSize, setBatchSize] = useState(5)

  // Opciones avanzadas
  const [showAdvanced, setShowAdvanced] = useState(false)
  const [printerOptions, setPrinterOptions] = useState<PrinterOption[]>([])
  const [optionValues, setOptionValues] = useState<Record<string, string>>({})
  const [optionsLoading, setOptionsLoading] = useState(false)
  const [orientation, setOrientation] = useState('')
  const [numberUp, setNumberUp] = useState('1')
  const [fitToPage, setFitToPage] = useState(false)

  // Estado de lotes
  const [currentBatch, setCurrentBatch] = useState(1)
  const [totalBatches, setTotalBatches] = useState(1)
  const [currentBatchCopies, setCurrentBatchCopies] = useState(1)
  const [completedCopies, setCompletedCopies] = useState(0)

  // Estado de impresion
  const [printingStep, setPrintingStep] = useState(false)

  const fileInputRef = useRef<HTMLInputElement>(null)
  const cleanupRef = useRef<string | null>(null)

  // Cleanup al desmontar
  useEffect(() => {
    return () => {
      if (cleanupRef.current) {
        duplexCleanup(cleanupRef.current).catch(() => {})
      }
    }
  }, [])

  // Opciones que el wizard controla internamente (no mostrar al usuario)
  const hiddenOptionKeys = ['page-set', 'outputorder', 'collate', 'orientation-requested', 'number-up', 'fit-to-page']
  const filteredPrinterOptions = printerOptions.filter(opt => !hiddenOptionKeys.includes(opt.key))

  async function loadPrinterOptions(printerName: string) {
    setOptionsLoading(true)
    try {
      const opts = await fetchPrinterOptions(printerName)
      setPrinterOptions(opts)
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

  // Cargar opciones al cambiar impresora (solo si estamos en configure)
  useEffect(() => {
    if (step === 'configure' && printer) {
      loadPrinterOptions(printer)
    }
  }, [printer, step])

  const handleFileSelect = useCallback(async (selectedFile: File) => {
    setError(null)
    setPreparing(true)

    try {
      const data = await duplexPrepare(selectedFile)
      setPrepareData(data)
      cleanupRef.current = data.temp_id
      setStep('configure')
    } catch (err: any) {
      setError(err.message || 'Error al preparar archivo')
    } finally {
      setPreparing(false)
    }
  }, [])

  const handleFileInput = (e: React.ChangeEvent<HTMLInputElement>) => {
    const f = e.target.files?.[0]
    if (f) handleFileSelect(f)
    e.target.value = ''
  }

  const handleDrop = (e: React.DragEvent) => {
    e.preventDefault()
    const f = e.dataTransfer.files[0]
    if (f) handleFileSelect(f)
  }

  const calculateBatches = useCallback(() => {
    const batches = Math.ceil(totalCopies / batchSize)
    setTotalBatches(batches)
  }, [totalCopies, batchSize])

  useEffect(() => {
    calculateBatches()
  }, [calculateBatches])

  function buildOptions(): Record<string, string> {
    const opts: Record<string, string> = {}
    // Opciones dinamicas de la impresora que difieren del default
    for (const opt of printerOptions) {
      const current = optionValues[opt.key]
      if (current && current !== opt.default_value) {
        opts[opt.key] = current
      }
    }
    // Opciones estandar
    if (orientation) opts['orientation-requested'] = orientation
    if (numberUp !== '1') opts['number-up'] = numberUp
    if (fitToPage) opts['fit-to-page'] = 'true'
    return opts
  }

  const startPrinting = () => {
    setCurrentBatch(1)
    setCompletedCopies(0)

    const copiesThisBatch = Math.min(batchSize, totalCopies)
    setCurrentBatchCopies(copiesThisBatch)
    setStep('printing-odd')
    printOddPages(copiesThisBatch)
  }

  const printOddPages = async (copies: number) => {
    if (!prepareData) return
    setPrintingStep(true)
    setError(null)

    try {
      await duplexPrintStep({
        temp_id: prepareData.temp_id,
        printer,
        copies,
        page_set: 'odd',
        options: buildOptions(),
      })
      setStep('flip')
    } catch (err: any) {
      setError(err.message || 'Error al imprimir paginas impares')
    } finally {
      setPrintingStep(false)
    }
  }

  const printEvenPages = async () => {
    if (!prepareData) return
    setPrintingStep(true)
    setError(null)

    try {
      await duplexPrintStep({
        temp_id: prepareData.temp_id,
        printer,
        copies: currentBatchCopies,
        page_set: 'even',
        output_order: 'reverse',
        options: buildOptions(),
      })

      const newCompleted = completedCopies + currentBatchCopies
      setCompletedCopies(newCompleted)

      if (newCompleted >= totalCopies) {
        // Todas las copias hechas
        setStep('complete')
      } else {
        // Mas lotes pendientes
        setStep('batch-done')
      }
    } catch (err: any) {
      setError(err.message || 'Error al imprimir paginas pares')
    } finally {
      setPrintingStep(false)
    }
  }

  const startNextBatch = () => {
    const nextBatch = currentBatch + 1
    setCurrentBatch(nextBatch)

    const remaining = totalCopies - completedCopies
    const copiesThisBatch = Math.min(batchSize, remaining)
    setCurrentBatchCopies(copiesThisBatch)

    setStep('printing-odd')
    printOddPages(copiesThisBatch)
  }

  const handleClose = () => {
    if (cleanupRef.current) {
      duplexCleanup(cleanupRef.current).catch(() => {})
      cleanupRef.current = null
    }
    onClose()
  }

  const handleComplete = () => {
    if (cleanupRef.current) {
      duplexCleanup(cleanupRef.current).catch(() => {})
      cleanupRef.current = null
    }
    onComplete()
  }

  // Indicador de pasos
  const stepLabels = ['Archivo', 'Configurar', 'Impares', 'Voltear', 'Pares', 'Listo']
  const stepIndex = {
    upload: 0,
    configure: 1,
    'printing-odd': 2,
    flip: 3,
    'printing-even': 4,
    'batch-done': 4,
    complete: 5,
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
      <div
        className="rounded-xl p-6 w-full max-w-lg mx-4 max-h-[90vh] overflow-y-auto"
        style={{ backgroundColor: 'var(--bg-secondary)', border: '1px solid var(--border)' }}
      >
        {/* Header */}
        <div className="flex items-center justify-between mb-4">
          <div className="flex items-center gap-2">
            <BookOpen size={20} style={{ color: 'var(--accent)' }} />
            <h3 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>
              Doble cara manual
            </h3>
          </div>
          <button onClick={handleClose} style={{ color: 'var(--text-secondary)' }}>
            <X size={20} />
          </button>
        </div>

        {/* Progress bar */}
        <div className="flex items-center gap-1 mb-6">
          {stepLabels.map((label, i) => (
            <div key={label} className="flex-1">
              <div
                className="h-1.5 rounded-full transition-all duration-300"
                style={{
                  backgroundColor: i <= stepIndex[step] ? 'var(--accent)' : 'var(--border)',
                }}
              />
              <div
                className="text-[9px] text-center mt-1"
                style={{ color: i <= stepIndex[step] ? 'var(--accent)' : 'var(--text-secondary)', opacity: i <= stepIndex[step] ? 1 : 0.5 }}
              >
                {label}
              </div>
            </div>
          ))}
        </div>

        {/* Error */}
        {error && (
          <div
            className="rounded-lg p-3 mb-4 flex items-start gap-2"
            style={{ backgroundColor: 'var(--danger-alpha, rgba(239,68,68,0.1))', border: '1px solid var(--danger)' }}
          >
            <AlertTriangle size={16} className="flex-shrink-0 mt-0.5" style={{ color: 'var(--danger)' }} />
            <div className="flex-1">
              <p className="text-sm" style={{ color: 'var(--danger)' }}>{error}</p>
              {(step === 'printing-odd' || step === 'printing-even') && (
                <button
                  onClick={() => {
                    setError(null)
                    if (step === 'printing-odd') printOddPages(currentBatchCopies)
                    else printEvenPages()
                  }}
                  className="mt-2 flex items-center gap-1 text-xs px-3 py-1.5 rounded-lg"
                  style={{ backgroundColor: 'var(--danger)', color: '#fff' }}
                >
                  <RotateCcw size={12} /> Reintentar
                </button>
              )}
            </div>
          </div>
        )}

        {/* Step: Upload */}
        {step === 'upload' && (
          <div>
            <div
              className="rounded-xl p-8 text-center cursor-pointer transition-all duration-200 hover:shadow-lg"
              style={{
                backgroundColor: 'var(--card-bg)',
                border: '2px dashed var(--card-border)',
              }}
              onClick={() => fileInputRef.current?.click()}
              onDragOver={(e) => e.preventDefault()}
              onDrop={handleDrop}
            >
              {preparing ? (
                <div>
                  <Loader2 size={36} className="mx-auto mb-3 animate-spin" style={{ color: 'var(--accent)' }} />
                  <p className="text-sm" style={{ color: 'var(--text-secondary)' }}>
                    Preparando archivo...
                  </p>
                </div>
              ) : (
                <div>
                  <FileText size={36} className="mx-auto mb-3" style={{ color: 'var(--text-secondary)' }} />
                  <p className="text-sm font-medium" style={{ color: 'var(--text-primary)' }}>
                    Arrastra un archivo o haz clic para seleccionar
                  </p>
                  <p className="text-xs mt-1" style={{ color: 'var(--text-secondary)' }}>
                    PDF, imagenes, texto plano
                  </p>
                </div>
              )}
            </div>
            <input
              ref={fileInputRef}
              type="file"
              accept=".pdf,.ps,.eps,.txt,.png,.jpg,.jpeg,.gif,.tiff,.tif,.bmp,.svg"
              className="hidden"
              onChange={handleFileInput}
            />
          </div>
        )}

        {/* Step: Configure */}
        {step === 'configure' && prepareData && (
          <div className="space-y-4">
            {/* Info del archivo */}
            <div className="rounded-lg p-3" style={{ backgroundColor: 'var(--bg-tertiary)' }}>
              <div className="flex items-center gap-2">
                <FileText size={16} style={{ color: 'var(--accent)' }} />
                <span className="text-sm font-medium truncate" style={{ color: 'var(--text-primary)' }}>
                  {prepareData.filename}
                </span>
                <span className="text-xs ml-auto" style={{ color: 'var(--text-secondary)' }}>
                  {prepareData.page_count} {prepareData.page_count === 1 ? 'pagina' : 'paginas'}
                </span>
              </div>
            </div>

            {/* Selector de impresora */}
            <div>
              <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>
                Impresora
              </label>
              <select
                value={printer}
                onChange={(e) => setPrinter(e.target.value)}
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

            {/* Copias */}
            <div className="grid grid-cols-2 gap-3">
              <div>
                <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>
                  Copias totales
                </label>
                <input
                  type="number"
                  min={1}
                  max={100}
                  value={totalCopies}
                  onChange={(e) => setTotalCopies(Math.max(1, Math.min(100, parseInt(e.target.value) || 1)))}
                  className="w-full px-3 py-2 rounded-lg text-sm outline-none"
                  style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
                />
              </div>
              <div>
                <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>
                  Copias por lote
                </label>
                <select
                  value={batchSize}
                  onChange={(e) => setBatchSize(parseInt(e.target.value))}
                  className="w-full px-3 py-2 rounded-lg text-sm outline-none cursor-pointer"
                  style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
                >
                  <option value={1}>1 copia</option>
                  <option value={3}>3 copias</option>
                  <option value={5}>5 copias</option>
                  <option value={10}>10 copias</option>
                </select>
              </div>
            </div>

            {/* Info de lotes */}
            {totalCopies > 1 && (
              <div className="rounded-lg p-3" style={{ backgroundColor: 'var(--bg-tertiary)' }}>
                <div className="flex items-center gap-2">
                  <Layers size={14} style={{ color: 'var(--accent)' }} />
                  <span className="text-xs" style={{ color: 'var(--text-secondary)' }}>
                    Se imprimiran en <strong style={{ color: 'var(--text-primary)' }}>{totalBatches} {totalBatches === 1 ? 'lote' : 'lotes'}</strong>.
                    {totalBatches > 1 && ' Imprimir por lotes reduce el riesgo si hay un atasco de papel.'}
                  </span>
                </div>
              </div>
            )}

            {/* Configuracion avanzada */}
            <div>
              <button
                onClick={() => setShowAdvanced(!showAdvanced)}
                className="flex items-center gap-2 text-xs font-medium hover:opacity-80 transition-opacity"
                style={{ color: 'var(--text-secondary)' }}
              >
                {showAdvanced ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
                <Settings2 size={14} style={{ color: 'var(--accent)' }} />
                Configuracion avanzada
              </button>

              {showAdvanced && (
                <div className="mt-3 space-y-3 rounded-lg p-3" style={{ backgroundColor: 'var(--bg-tertiary)', border: '1px solid var(--border)' }}>
                  {/* Opciones estandar */}
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
                    </div>
                  </div>
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

                  {/* Opciones dinamicas de la impresora */}
                  {optionsLoading ? (
                    <div className="flex items-center justify-center py-3">
                      <Loader2 size={14} className="animate-spin" style={{ color: 'var(--accent)' }} />
                      <span className="text-xs ml-2" style={{ color: 'var(--text-secondary)' }}>Cargando opciones...</span>
                    </div>
                  ) : filteredPrinterOptions.length > 0 && (
                    <>
                      <div className="flex items-center gap-2 pt-2" style={{ borderTop: '1px solid var(--border)' }}>
                        <span className="text-[10px] font-medium" style={{ color: 'var(--text-secondary)' }}>
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
                    </>
                  )}
                </div>
              )}
            </div>

            {/* Boton iniciar */}
            <button
              onClick={startPrinting}
              className="w-full flex items-center justify-center gap-2 px-4 py-3 rounded-lg text-sm font-medium transition-all duration-200 hover:opacity-90"
              style={{ backgroundColor: 'var(--accent)', color: '#ffffff' }}
            >
              <Printer size={16} />
              Iniciar impresion doble cara
            </button>
          </div>
        )}

        {/* Step: Printing odd pages */}
        {step === 'printing-odd' && (
          <div className="text-center py-6">
            {totalBatches > 1 && (
              <div className="text-xs font-medium mb-4 px-3 py-1.5 rounded-full inline-block" style={{ backgroundColor: 'var(--accent-alpha)', color: 'var(--accent)' }}>
                Lote {currentBatch} de {totalBatches} ({currentBatchCopies} {currentBatchCopies === 1 ? 'copia' : 'copias'})
              </div>
            )}
            {printingStep && !error && (
              <>
                <Loader2 size={40} className="mx-auto mb-4 animate-spin" style={{ color: 'var(--accent)' }} />
                <p className="text-sm font-medium" style={{ color: 'var(--text-primary)' }}>
                  Imprimiendo paginas impares...
                </p>
                <p className="text-xs mt-1" style={{ color: 'var(--text-secondary)' }}>
                  {currentBatchCopies} {currentBatchCopies === 1 ? 'copia' : 'copias'} - solo paginas 1, 3, 5...
                </p>
              </>
            )}
          </div>
        )}

        {/* Step: Flip pages */}
        {step === 'flip' && (
          <div className="space-y-4">
            {totalBatches > 1 && (
              <div className="text-center">
                <span className="text-xs font-medium px-3 py-1.5 rounded-full inline-block" style={{ backgroundColor: 'var(--accent-alpha)', color: 'var(--accent)' }}>
                  Lote {currentBatch} de {totalBatches}
                </span>
              </div>
            )}

            <div
              className="rounded-xl p-5"
              style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}
            >
              <h4 className="text-sm font-semibold mb-4 text-center" style={{ color: 'var(--warning)' }}>
                Voltear las hojas
              </h4>

              <div className="space-y-3">
                <div className="flex items-start gap-3">
                  <div
                    className="flex-shrink-0 w-6 h-6 rounded-full flex items-center justify-center text-xs font-bold"
                    style={{ backgroundColor: 'var(--accent)', color: '#fff' }}
                  >
                    1
                  </div>
                  <p className="text-sm pt-0.5" style={{ color: 'var(--text-primary)' }}>
                    Retire <strong>todas</strong> las hojas impresas de la bandeja de salida
                  </p>
                </div>

                <div className="flex items-start gap-3">
                  <div
                    className="flex-shrink-0 w-6 h-6 rounded-full flex items-center justify-center text-xs font-bold"
                    style={{ backgroundColor: 'var(--accent)', color: '#fff' }}
                  >
                    2
                  </div>
                  <div className="pt-0.5">
                    <p className="text-sm" style={{ color: 'var(--text-primary)' }}>
                      Voltee el paquete de hojas completo
                    </p>
                    <p className="text-xs mt-0.5" style={{ color: 'var(--text-secondary)' }}>
                      El lado impreso debe quedar hacia abajo
                    </p>
                  </div>
                </div>

                <div className="flex items-start gap-3">
                  <div
                    className="flex-shrink-0 w-6 h-6 rounded-full flex items-center justify-center text-xs font-bold"
                    style={{ backgroundColor: 'var(--accent)', color: '#fff' }}
                  >
                    3
                  </div>
                  <p className="text-sm pt-0.5" style={{ color: 'var(--text-primary)' }}>
                    Coloque las hojas en la bandeja de entrada en el mismo orden
                  </p>
                </div>
              </div>

              {/* Diagrama visual simple */}
              <div
                className="mt-4 p-3 rounded-lg text-center"
                style={{ backgroundColor: 'var(--bg-tertiary)' }}
              >
                <div className="flex items-center justify-center gap-4">
                  <div className="text-center">
                    <div
                      className="w-12 h-16 rounded border-2 flex items-center justify-center mx-auto mb-1"
                      style={{ borderColor: 'var(--accent)', backgroundColor: 'var(--accent-alpha)' }}
                    >
                      <ArrowUp size={16} style={{ color: 'var(--accent)' }} />
                    </div>
                    <span className="text-[10px]" style={{ color: 'var(--text-secondary)' }}>Impreso arriba</span>
                  </div>
                  <div style={{ color: 'var(--accent)' }}>
                    <RotateCcw size={20} />
                  </div>
                  <div className="text-center">
                    <div
                      className="w-12 h-16 rounded border-2 flex items-center justify-center mx-auto mb-1"
                      style={{ borderColor: 'var(--success)', backgroundColor: 'var(--success-alpha, rgba(34,197,94,0.1))' }}
                    >
                      <ArrowDown size={16} style={{ color: 'var(--success)' }} />
                    </div>
                    <span className="text-[10px]" style={{ color: 'var(--text-secondary)' }}>Impreso abajo</span>
                  </div>
                </div>
              </div>
            </div>

            <button
              onClick={() => {
                setStep('printing-even')
                printEvenPages()
              }}
              className="w-full flex items-center justify-center gap-2 px-4 py-3 rounded-lg text-sm font-medium transition-all duration-200 hover:opacity-90"
              style={{ backgroundColor: 'var(--accent)', color: '#ffffff' }}
            >
              <Check size={16} />
              Listo, hojas volteadas
            </button>
          </div>
        )}

        {/* Step: Printing even pages */}
        {step === 'printing-even' && (
          <div className="text-center py-6">
            {totalBatches > 1 && (
              <div className="text-xs font-medium mb-4 px-3 py-1.5 rounded-full inline-block" style={{ backgroundColor: 'var(--accent-alpha)', color: 'var(--accent)' }}>
                Lote {currentBatch} de {totalBatches} ({currentBatchCopies} {currentBatchCopies === 1 ? 'copia' : 'copias'})
              </div>
            )}
            {printingStep && !error && (
              <>
                <Loader2 size={40} className="mx-auto mb-4 animate-spin" style={{ color: 'var(--accent)' }} />
                <p className="text-sm font-medium" style={{ color: 'var(--text-primary)' }}>
                  Imprimiendo paginas pares en reversa...
                </p>
                <p className="text-xs mt-1" style={{ color: 'var(--text-secondary)' }}>
                  {currentBatchCopies} {currentBatchCopies === 1 ? 'copia' : 'copias'} - paginas 2, 4, 6... en orden inverso
                </p>
              </>
            )}
          </div>
        )}

        {/* Step: Batch done */}
        {step === 'batch-done' && (
          <div className="text-center py-6 space-y-4">
            <div
              className="w-14 h-14 rounded-full flex items-center justify-center mx-auto"
              style={{ backgroundColor: 'var(--success-alpha, rgba(34,197,94,0.1))', color: 'var(--success)' }}
            >
              <Check size={28} />
            </div>
            <div>
              <p className="text-sm font-medium" style={{ color: 'var(--text-primary)' }}>
                Lote {currentBatch} completado
              </p>
              <p className="text-xs mt-1" style={{ color: 'var(--text-secondary)' }}>
                {completedCopies} de {totalCopies} copias completadas.
                Faltan {totalCopies - completedCopies} copias ({totalBatches - currentBatch} {totalBatches - currentBatch === 1 ? 'lote' : 'lotes'} mas).
              </p>
            </div>

            {/* Barra de progreso */}
            <div className="w-full rounded-full h-2" style={{ backgroundColor: 'var(--border)' }}>
              <div
                className="h-2 rounded-full transition-all duration-500"
                style={{
                  backgroundColor: 'var(--accent)',
                  width: `${(completedCopies / totalCopies) * 100}%`,
                }}
              />
            </div>

            <button
              onClick={startNextBatch}
              className="w-full flex items-center justify-center gap-2 px-4 py-3 rounded-lg text-sm font-medium transition-all duration-200 hover:opacity-90"
              style={{ backgroundColor: 'var(--accent)', color: '#ffffff' }}
            >
              <Printer size={16} />
              Iniciar lote {currentBatch + 1}
            </button>
          </div>
        )}

        {/* Step: Complete */}
        {step === 'complete' && (
          <div className="text-center py-6 space-y-4">
            <div
              className="w-16 h-16 rounded-full flex items-center justify-center mx-auto"
              style={{ backgroundColor: 'var(--success-alpha, rgba(34,197,94,0.1))', color: 'var(--success)' }}
            >
              <Check size={32} />
            </div>
            <div>
              <p className="text-base font-semibold" style={{ color: 'var(--text-primary)' }}>
                Impresion doble cara completada
              </p>
              <p className="text-sm mt-1" style={{ color: 'var(--text-secondary)' }}>
                Se imprimieron {totalCopies} {totalCopies === 1 ? 'copia' : 'copias'} a doble cara exitosamente
                {totalBatches > 1 && ` en ${totalBatches} lotes`}.
              </p>
            </div>

            <button
              onClick={handleComplete}
              className="w-full flex items-center justify-center gap-2 px-4 py-3 rounded-lg text-sm font-medium transition-all duration-200 hover:opacity-90"
              style={{ backgroundColor: 'var(--accent)', color: '#ffffff' }}
            >
              <Check size={16} />
              Cerrar
            </button>
          </div>
        )}
      </div>
    </div>
  )
}

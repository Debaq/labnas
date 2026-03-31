import { useEffect, useState, useRef, useCallback } from 'react'
import { useNavigate } from 'react-router-dom'
import {
  Box,
  ArrowLeft,
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
  ArrowRight,
  ExternalLink,
  Calculator,
  DollarSign,
  Pencil,
} from 'lucide-react'
import {
  fetchPrinters3D,
  addPrinter3D,
  deletePrinter3D,
  updatePrinter3D,
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

// ── Calculadora de costos 3D ──

interface HardwareItem {
  id: string
  name: string
  cost: number | string
  qty: number | string
}

function CostCalculator() {
  // Material
  const [materialWeight, setMaterialWeight] = useState<number | string>('')
  const [materialPriceKg, setMaterialPriceKg] = useState<number | string>('')
  const [materialDensity, setMaterialDensity] = useState<number | string>('')
  const [stlVolume, setStlVolume] = useState<number | null>(null)
  const [stlFileName, setStlFileName] = useState('')

  // Parse binary STL and calculate volume in cm³
  function parseSTL(buffer: ArrayBuffer): number {
    const view = new DataView(buffer)
    const numTriangles = view.getUint32(80, true)
    let volume = 0
    for (let i = 0; i < numTriangles; i++) {
      const offset = 84 + i * 50 + 12 // skip header + normal
      const x1 = view.getFloat32(offset, true), y1 = view.getFloat32(offset + 4, true), z1 = view.getFloat32(offset + 8, true)
      const x2 = view.getFloat32(offset + 12, true), y2 = view.getFloat32(offset + 16, true), z2 = view.getFloat32(offset + 20, true)
      const x3 = view.getFloat32(offset + 24, true), y3 = view.getFloat32(offset + 28, true), z3 = view.getFloat32(offset + 32, true)
      // Signed volume of tetrahedron with origin
      volume += (x1 * (y2 * z3 - y3 * z2) - y1 * (x2 * z3 - x3 * z2) + z1 * (x2 * y3 - x3 * y2)) / 6
    }
    return Math.abs(volume) / 1000 // mm³ to cm³
  }

  function handleSTLFile(e: React.ChangeEvent<HTMLInputElement>) {
    const file = e.target.files?.[0]
    if (!file) return
    const reader = new FileReader()
    reader.onload = () => {
      const vol = parseSTL(reader.result as ArrayBuffer)
      setStlVolume(vol)
      setStlFileName(file.name)
      // Auto-calculate weight if density is set
      const d = Number(materialDensity)
      if (d > 0) setMaterialWeight(Math.round(vol * d))
    }
    reader.readAsArrayBuffer(file)
    e.target.value = ''
  }

  // Electricidad
  const [printHours, setPrintHours] = useState<number | string>('')
  const [printerWatts, setPrinterWatts] = useState<number | string>('')
  const [kwhPrice, setKwhPrice] = useState<number | string>('')

  // Maquina
  const [machineCost, setMachineCost] = useState<number | string>('')
  const [machineLifeHours, setMachineLifeHours] = useState<number | string>('')

  // Tiempo de trabajo
  const [designHours, setDesignHours] = useState<number | string>('')
  const [prepHours, setPrepHours] = useState<number | string>('')
  const [postProcessHours, setPostProcessHours] = useState<number | string>('')
  const [hourlyRate, setHourlyRate] = useState<number | string>('')

  // Hardware extra
  const [hardwareItems, setHardwareItems] = useState<HardwareItem[]>([])

  // Factores
  const [failRate, setFailRate] = useState<number | string>('')
  const [margin, setMargin] = useState<number | string>('')
  const [quantity, setQuantity] = useState<number | string>(1)

  function n(v: number | string): number { return Number(v) || 0 }

  const materialCost = (n(materialWeight) / 1000) * n(materialPriceKg)
  const electricityCost = (n(printHours) * n(printerWatts) / 1000) * n(kwhPrice)
  const depreciationCost = n(machineLifeHours) > 0 ? (n(machineCost) / n(machineLifeHours)) * n(printHours) : 0
  const laborCost = (n(designHours) + n(prepHours) + n(postProcessHours)) * n(hourlyRate)
  const hardwareCost = hardwareItems.reduce((sum, item) => sum + n(item.cost) * n(item.qty), 0)

  const subtotal = materialCost + electricityCost + depreciationCost + laborCost + hardwareCost
  const failAdjusted = n(failRate) > 0 ? subtotal * (1 + n(failRate) / 100) : subtotal
  const withMargin = n(margin) > 0 ? failAdjusted * (1 + n(margin) / 100) : failAdjusted
  const totalPerUnit = withMargin
  const totalAll = totalPerUnit * Math.max(n(quantity), 1)

  const hasAnyCost = subtotal > 0

  function addHardwareItem() {
    setHardwareItems(prev => [...prev, { id: crypto.randomUUID(), name: '', cost: '', qty: 1 }])
  }

  function updateHardwareItem(id: string, field: keyof HardwareItem, value: string | number) {
    setHardwareItems(prev => prev.map(item => item.id === id ? { ...item, [field]: value } : item))
  }

  function removeHardwareItem(id: string) {
    setHardwareItems(prev => prev.filter(item => item.id !== id))
  }

  function resetAll() {
    setMaterialWeight(''); setMaterialPriceKg('')
    setPrintHours(''); setPrinterWatts(''); setKwhPrice('')
    setMachineCost(''); setMachineLifeHours('')
    setDesignHours(''); setPrepHours(''); setPostProcessHours(''); setHourlyRate('')
    setHardwareItems([])
    setFailRate(''); setMargin(''); setQuantity(1)
  }

  const inputStyle = { backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }
  const inputClass = "w-full px-3 py-2 rounded-lg text-sm outline-none"

  function NumField({ label, hint, value, onChange, unit, placeholder }: {
    label: string; hint?: string; value: number | string; onChange: (v: number | string) => void; unit?: string; placeholder?: string
  }) {
    return (
      <div>
        <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>{label}</label>
        <div className="relative">
          <input
            type="number"
            min={0}
            step="any"
            value={value}
            onChange={(e) => onChange(e.target.value === '' ? '' : parseFloat(e.target.value))}
            placeholder={placeholder || '0'}
            className={inputClass + (unit ? ' pr-10' : '')}
            style={inputStyle}
          />
          {unit && (
            <span className="absolute right-3 top-1/2 -translate-y-1/2 text-xs pointer-events-none" style={{ color: 'var(--text-secondary)', opacity: 0.6 }}>{unit}</span>
          )}
        </div>
        {hint && <span className="block text-[10px] mt-0.5" style={{ color: 'var(--text-secondary)', opacity: 0.7 }}>{hint}</span>}
      </div>
    )
  }

  function CostLine({ label, value, highlight }: { label: string; value: number; highlight?: boolean }) {
    if (value <= 0) return null
    return (
      <div className="flex justify-between items-center py-1">
        <span className="text-xs" style={{ color: highlight ? 'var(--text-primary)' : 'var(--text-secondary)' }}>{label}</span>
        <span className={'text-sm font-mono' + (highlight ? ' font-bold' : '')} style={{ color: highlight ? 'var(--accent)' : 'var(--text-primary)' }}>
          ${value.toFixed(2)}
        </span>
      </div>
    )
  }

  function SectionHeader({ title }: { title: string }) {
    return (
      <div className="flex items-center gap-2 mb-2 mt-1">
        <span className="text-xs font-semibold uppercase tracking-wider" style={{ color: 'var(--accent)' }}>{title}</span>
        <div className="flex-1 h-px" style={{ backgroundColor: 'var(--border)' }} />
      </div>
    )
  }

  return (
    <div
      className="rounded-xl p-5"
      style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}
    >
      <div className="flex items-center justify-between mb-4">
        <div className="flex items-center gap-2">
          <Calculator size={18} style={{ color: 'var(--accent)' }} />
          <h3 className="text-sm font-semibold" style={{ color: 'var(--text-primary)' }}>Calculadora de costos</h3>
        </div>
        {hasAnyCost && (
          <button onClick={resetAll} className="text-xs px-2 py-1 rounded-lg" style={{ color: 'var(--text-secondary)', border: '1px solid var(--border)' }}>
            Limpiar
          </button>
        )}
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-[1fr_280px] gap-6">
        {/* Columna izquierda: inputs */}
        <div className="space-y-4">
          {/* Material */}
          <div>
            <SectionHeader title="Material" />
            {/* STL auto-weight */}
            <div className="flex items-center gap-2 mb-2">
              <label className="flex items-center gap-2 px-3 py-1.5 rounded-lg text-xs font-medium cursor-pointer"
                style={{ color: 'var(--accent)', border: '1px dashed var(--border)' }}>
                <Upload size={12} /> Cargar STL para estimar peso
                <input type="file" accept=".stl" className="hidden" onChange={handleSTLFile} />
              </label>
              {stlVolume !== null && (
                <span className="text-[10px]" style={{ color: 'var(--text-secondary)' }}>
                  {stlFileName}: {stlVolume.toFixed(2)} cm3
                  {Number(materialDensity) > 0 && ` = ${(stlVolume * Number(materialDensity)).toFixed(0)}g`}
                </span>
              )}
            </div>
            <div className="grid grid-cols-3 gap-3">
              <NumField label="Peso del material" value={materialWeight} onChange={setMaterialWeight} unit="g" hint="Del slicer o STL" />
              <NumField label="Precio filamento" value={materialPriceKg} onChange={setMaterialPriceKg} unit="$/kg" />
              <NumField label="Densidad" value={materialDensity} onChange={(v) => {
                setMaterialDensity(v)
                if (stlVolume && Number(v) > 0) setMaterialWeight(Math.round(stlVolume * Number(v)))
              }} unit="g/cm3" hint="PLA=1.24 ABS=1.04" />
            </div>
          </div>

          {/* Electricidad + Maquina */}
          <div>
            <SectionHeader title="Impresora" />
            <div className="grid grid-cols-2 sm:grid-cols-3 gap-3">
              <NumField label="Tiempo de impresion" value={printHours} onChange={setPrintHours} unit="h" hint="Estimado del slicer" />
              <NumField label="Consumo" value={printerWatts} onChange={setPrinterWatts} unit="W" hint="Potencia de la impresora" />
              <NumField label="Precio electricidad" value={kwhPrice} onChange={setKwhPrice} unit="$/kWh" />
            </div>
            <div className="grid grid-cols-2 gap-3 mt-3">
              <NumField label="Costo de la impresora" value={machineCost} onChange={setMachineCost} unit="$" hint="Precio de compra" />
              <NumField label="Vida util estimada" value={machineLifeHours} onChange={setMachineLifeHours} unit="h" hint="Horas totales de uso esperado" />
            </div>
          </div>

          {/* Tiempo de trabajo */}
          <div>
            <SectionHeader title="Mano de obra" />
            <div className="grid grid-cols-2 sm:grid-cols-4 gap-3">
              <NumField label="Diseno" value={designHours} onChange={setDesignHours} unit="h" hint="Modelado 3D" />
              <NumField label="Preparacion" value={prepHours} onChange={setPrepHours} unit="h" hint="Slicing, calibracion" />
              <NumField label="Post-procesado" value={postProcessHours} onChange={setPostProcessHours} unit="h" hint="Lijado, pintura, curado" />
              <NumField label="Tarifa por hora" value={hourlyRate} onChange={setHourlyRate} unit="$/h" />
            </div>
          </div>

          {/* Hardware extra */}
          <div>
            <SectionHeader title="Hardware adicional" />
            <span className="block text-[10px] mb-2" style={{ color: 'var(--text-secondary)', opacity: 0.7 }}>Insertos, herrajes, tornillos, electrónica, etc.</span>
            {hardwareItems.map((item) => (
              <div key={item.id} className="flex items-center gap-2 mb-2">
                <input
                  type="text"
                  value={item.name}
                  onChange={(e) => updateHardwareItem(item.id, 'name', e.target.value)}
                  placeholder="Descripcion"
                  className="flex-1 px-3 py-1.5 rounded-lg text-sm outline-none"
                  style={inputStyle}
                />
                <input
                  type="number"
                  min={0}
                  step="any"
                  value={item.cost}
                  onChange={(e) => updateHardwareItem(item.id, 'cost', e.target.value === '' ? '' : parseFloat(e.target.value))}
                  placeholder="$/u"
                  className="w-20 px-2 py-1.5 rounded-lg text-sm outline-none text-right"
                  style={inputStyle}
                />
                <input
                  type="number"
                  min={1}
                  value={item.qty}
                  onChange={(e) => updateHardwareItem(item.id, 'qty', e.target.value === '' ? '' : parseInt(e.target.value))}
                  placeholder="Cant"
                  className="w-16 px-2 py-1.5 rounded-lg text-sm outline-none text-right"
                  style={inputStyle}
                />
                <button onClick={() => removeHardwareItem(item.id)} className="p-1 rounded-lg" style={{ color: 'var(--danger)' }}>
                  <Trash2 size={14} />
                </button>
              </div>
            ))}
            <button
              onClick={addHardwareItem}
              className="flex items-center gap-1 text-xs px-3 py-1.5 rounded-lg"
              style={{ color: 'var(--accent)', border: '1px dashed var(--border)' }}
            >
              <Plus size={12} /> Agregar item
            </button>
          </div>

          {/* Ajustes finales */}
          <div>
            <SectionHeader title="Ajustes" />
            <div className="grid grid-cols-3 gap-3">
              <NumField label="Tasa de fallo" value={failRate} onChange={setFailRate} unit="%" hint="Impresiones fallidas esperadas" />
              <NumField label="Margen de ganancia" value={margin} onChange={setMargin} unit="%" />
              <NumField label="Cantidad" value={quantity} onChange={setQuantity} placeholder="1" hint="Unidades a producir" />
            </div>
          </div>
        </div>

        {/* Columna derecha: resumen */}
        <div>
          <div
            className="rounded-xl p-4 sticky top-4"
            style={{ backgroundColor: 'var(--bg-tertiary)', border: '1px solid var(--border)' }}
          >
            <div className="flex items-center gap-2 mb-3">
              <DollarSign size={16} style={{ color: 'var(--accent)' }} />
              <span className="text-xs font-semibold uppercase tracking-wider" style={{ color: 'var(--accent)' }}>Resumen</span>
            </div>

            {!hasAnyCost ? (
              <p className="text-xs text-center py-4" style={{ color: 'var(--text-secondary)' }}>
                Completa los campos que necesites para calcular el costo
              </p>
            ) : (
              <div>
                <div className="space-y-0.5" style={{ borderBottom: '1px solid var(--border)', paddingBottom: '8px', marginBottom: '8px' }}>
                  <CostLine label="Material" value={materialCost} />
                  <CostLine label="Electricidad" value={electricityCost} />
                  <CostLine label="Depreciacion maquina" value={depreciationCost} />
                  <CostLine label="Mano de obra" value={laborCost} />
                  <CostLine label="Hardware adicional" value={hardwareCost} />
                </div>

                <CostLine label="Subtotal" value={subtotal} />
                {n(failRate) > 0 && <CostLine label={`+ Fallo (${n(failRate)}%)`} value={failAdjusted - subtotal} />}
                {n(margin) > 0 && <CostLine label={`+ Margen (${n(margin)}%)`} value={withMargin - failAdjusted} />}

                <div className="mt-3 pt-3" style={{ borderTop: '2px solid var(--accent)' }}>
                  <CostLine label="Costo por unidad" value={totalPerUnit} highlight />
                  {n(quantity) > 1 && (
                    <CostLine label={`Total (${n(quantity)} unidades)`} value={totalAll} highlight />
                  )}
                </div>
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  )
}

export default function Printers3DPage() {
  const navigate = useNavigate()
  const [printers, setPrinters] = useState<Printer3DConfig[]>([])
  const [statuses, setStatuses] = useState<Record<string, Printer3DStatus>>({})
  const [loading, setLoading] = useState(true)
  const [showAddModal, setShowAddModal] = useState(false)
  const [detecting, setDetecting] = useState(false)
  const [detected, setDetected] = useState<DetectPrintersResult[]>([])
  const [uploading, setUploading] = useState<string | null>(null)
  const [dragOver, setDragOver] = useState<string | null>(null)
  const [expandedPrinter, setExpandedPrinter] = useState<string | null>(null)
  const [selectedPrinterId, setSelectedPrinterId] = useState<string | null>(null)
  const [printerFiles, setPrinterFiles] = useState<Record<string, PrinterFileInfo[]>>({})
  const [loadingFiles, setLoadingFiles] = useState<string | null>(null)
  const [jogDistance, setJogDistance] = useState(10)
  const [gcodeInput, setGcodeInput] = useState<Record<string, string>>({})
  const [preheatHotend, setPreheatHotend] = useState(200)
  const [preheatBed, setPreheatBed] = useState(60)
  const [cameraKey, setCameraKey] = useState(0)
  const [actionFeedback, setActionFeedback] = useState<Record<string, string>>({})
  const fileInputRefs = useRef<Record<string, HTMLInputElement | null>>({})

  function printerWebUrl(printer: Printer3DConfig): string | null {
    switch (printer.printer_type) {
      case 'OctoPrint': return `http://${printer.ip}:${printer.port}`
      case 'Moonraker': return `http://${printer.ip}`
      case 'CrealityStock': return `http://${printer.ip}`
      case 'FlashForge': return null
    }
  }

  // Form state (shared between add and edit)
  const [formName, setFormName] = useState('')
  const [formIp, setFormIp] = useState('')
  const [formPort, setFormPort] = useState(5000)
  const [formType, setFormType] = useState<'OctoPrint' | 'Moonraker' | 'CrealityStock' | 'FlashForge'>('Moonraker')
  const [formApiKey, setFormApiKey] = useState('')
  const [formCameraUrl, setFormCameraUrl] = useState('')
  const [editingPrinterId, setEditingPrinterId] = useState<string | null>(null)

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

  // handleAdd is now replaced by handleSave (supports both add and edit)

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
    setFormPort(7125)
    setFormType('Moonraker')
    setFormApiKey('')
    setFormCameraUrl('')
    setEditingPrinterId(null)
  }

  function openEditModal(printer: Printer3DConfig) {
    setFormName(printer.name)
    setFormIp(printer.ip)
    setFormPort(printer.port)
    setFormType(printer.printer_type)
    setFormApiKey(printer.api_key || '')
    setFormCameraUrl(printer.camera_url || '')
    setEditingPrinterId(printer.id)
    setShowAddModal(true)
  }

  async function handleSave() {
    if (!formName.trim() || !formIp.trim()) return
    try {
      if (editingPrinterId) {
        await updatePrinter3D(editingPrinterId, {
          name: formName,
          ip: formIp,
          port: formPort,
          printer_type: formType,
          api_key: formApiKey || null,
          camera_url: formCameraUrl || null,
        })
      } else {
        await addPrinter3D({
          name: formName,
          ip: formIp,
          port: formPort,
          printer_type: formType,
          api_key: formApiKey || null,
          camera_url: formCameraUrl || null,
        })
      }
      setShowAddModal(false)
      resetForm()
      await loadPrinters()
    } catch (err) {
      console.error('Error guardando impresora:', err)
    }
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

  const selectedPrinter = printers.find(p => p.id === selectedPrinterId) || null

  return (
    <>
    <div className="flex gap-0 h-full -m-8" style={{ minHeight: 'calc(100vh - 130px)' }}>
      {/* Left panel: printer list */}
      <div
        className="flex flex-col h-full shrink-0"
        style={{ width: '280px', backgroundColor: 'var(--bg-secondary)', borderRight: '1px solid var(--border)' }}
      >
        {/* Header */}
        <div className="p-4 space-y-2" style={{ borderBottom: '1px solid var(--border)' }}>
          <button
            onClick={() => navigate('/dashboard')}
            className="flex items-center gap-2 text-xs font-medium hover:opacity-80 transition-all"
            style={{ color: 'var(--text-secondary)' }}
          >
            <ArrowLeft size={14} /> Volver al menu
          </button>
          <div className="flex items-center gap-2">
            <button
              onClick={handleDetect}
              disabled={detecting}
              className="flex-1 flex items-center justify-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium"
              style={{ color: 'var(--text-primary)', border: '1px solid var(--border)' }}
            >
              {detecting ? <Loader2 size={12} className="animate-spin" /> : <Search size={12} />}
              Buscar
            </button>
            <button
              onClick={() => { resetForm(); setShowAddModal(true) }}
              className="flex-1 flex items-center justify-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium"
              style={{ backgroundColor: 'var(--accent)', color: '#fff' }}
            >
              <Plus size={12} /> Agregar
            </button>
          </div>
        </div>

        {/* Detected banner */}
        {detected.length > 0 && (() => {
          const newDetected = detected.filter(d => !printers.some(p => p.ip === d.ip && p.port === d.port))
          if (newDetected.length === 0) return null
          return (
            <div className="p-3 space-y-1" style={{ borderBottom: '1px solid var(--border)', backgroundColor: 'var(--accent-alpha)' }}>
              <div className="flex items-center justify-between">
                <span className="text-[10px] font-semibold" style={{ color: 'var(--accent)' }}>{newDetected.length} detectada{newDetected.length !== 1 ? 's' : ''}</span>
                <button onClick={() => setDetected([])} style={{ color: 'var(--text-secondary)' }}><X size={12} /></button>
              </div>
              {newDetected.map((d, i) => (
                <button key={i} onClick={() => fillFromDetected(d)} className="w-full text-left px-2 py-1.5 rounded-lg text-xs hover:opacity-80"
                  style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--border)' }}>
                  <span className="font-mono">{d.ip}</span>
                  <span className="ml-1.5 text-[10px]" style={{ color: 'var(--accent)' }}>{d.printer_type}</span>
                </button>
              ))}
            </div>
          )
        })()}

        {/* Printer list */}
        <div className="flex-1 overflow-y-auto">
          {loading ? (
            <div className="flex justify-center py-8"><Loader2 size={20} className="animate-spin" style={{ color: 'var(--accent)' }} /></div>
          ) : printers.length === 0 ? (
            <div className="text-center py-8 px-4">
              <Box size={28} className="mx-auto mb-2" style={{ color: 'var(--text-secondary)', opacity: 0.4 }} />
              <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>Sin impresoras</p>
            </div>
          ) : printers.map(printer => {
            const status = statuses[printer.id]
            const isOnline = status?.online ?? false
            const job = status?.current_job
            const isPrinting = job && job.state.toLowerCase().includes('print')
            const isPaused = job && job.state.toLowerCase().includes('paus')
            const isSelected = selectedPrinterId === printer.id
            return (
              <button
                key={printer.id}
                onClick={() => setSelectedPrinterId(printer.id)}
                className="w-full text-left p-3 transition-all duration-150 hover:opacity-90"
                style={{
                  backgroundColor: isSelected ? 'var(--accent-alpha)' : 'transparent',
                  borderBottom: '1px solid var(--border)',
                  borderLeft: isSelected ? '3px solid var(--accent)' : '3px solid transparent',
                }}
              >
                <div className="flex items-center gap-2 mb-1">
                  <span className="w-2 h-2 rounded-full shrink-0" style={{ backgroundColor: isOnline ? 'var(--success)' : 'var(--danger)' }} />
                  <span className="text-sm font-medium truncate" style={{ color: 'var(--text-primary)' }}>{printer.name}</span>
                </div>
                {isOnline && job && (isPrinting || isPaused) ? (
                  <div className="ml-4">
                    <div className="flex items-center justify-between text-[10px] mb-0.5">
                      <span className="truncate" style={{ color: isPaused ? 'var(--warning)' : 'var(--accent)' }}>
                        {isPaused ? 'Pausada' : 'Imprimiendo'}
                      </span>
                      <span className="font-mono font-bold" style={{ color: 'var(--text-primary)' }}>{job.progress.toFixed(0)}%</span>
                    </div>
                    <div className="w-full h-1 rounded-full" style={{ backgroundColor: 'var(--bg-tertiary)' }}>
                      <div className="h-full rounded-full transition-all duration-500" style={{ width: `${job.progress}%`, backgroundColor: isPaused ? 'var(--warning)' : 'var(--accent)' }} />
                    </div>
                    <div className="flex justify-between text-[9px] mt-0.5" style={{ color: 'var(--text-secondary)' }}>
                      <span className="truncate mr-2">{job.file_name}</span>
                      {job.time_remaining != null && <span className="shrink-0">{formatTime(job.time_remaining)}</span>}
                    </div>
                  </div>
                ) : (
                  <span className="text-[10px] ml-4" style={{ color: isOnline ? 'var(--success)' : 'var(--text-secondary)' }}>
                    {isOnline ? 'Libre' : 'Desconectada'}
                  </span>
                )}
              </button>
            )
          })}
        </div>
      </div>

      {/* Right panel: selected printer or empty state */}
      <div className="flex-1 overflow-y-auto p-6">
        {!selectedPrinter ? (
          <div className="flex flex-col items-center justify-center h-full">
            <Box size={48} className="mb-3" style={{ color: 'var(--text-secondary)', opacity: 0.3 }} />
            <p className="text-sm" style={{ color: 'var(--text-secondary)' }}>
              {printers.length === 0 ? 'Agrega una impresora para empezar' : 'Selecciona una impresora'}
            </p>
          </div>
        ) : (() => {
          const printer = selectedPrinter
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
              <>
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
                    onClick={() => openEditModal(printer)}
                    className="p-1.5 rounded-lg transition-all duration-200 hover:opacity-80"
                    style={{ color: 'var(--text-secondary)' }}
                    title="Editar"
                  >
                    <Pencil size={14} />
                  </button>
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

            {/* Cost Calculator */}
            <div className="mt-6">
              <CostCalculator />
            </div>
          </>
        )
      })()}
      </div>
    </div>

      {/* Add Modal */}
      {showAddModal && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
          <div
            className="rounded-xl p-6 w-full max-w-md mx-4 max-h-[90vh] overflow-y-auto"
            style={{ backgroundColor: 'var(--bg-secondary)', border: '1px solid var(--border)' }}
          >
            <div className="flex items-center justify-between mb-6">
              <h3 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>
                {editingPrinterId ? 'Editar Impresora 3D' : 'Agregar Impresora 3D'}
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
                    const t = e.target.value as 'OctoPrint' | 'Moonraker' | 'CrealityStock' | 'FlashForge'
                    setFormType(t)
                    setFormPort(
                      t === 'Moonraker' ? 7125
                        : t === 'CrealityStock' ? 9999
                        : t === 'FlashForge' ? 8899
                        : 5000
                    )
                  }}
                  className="w-full px-3 py-2 rounded-lg text-sm outline-none cursor-pointer"
                  style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
                >
                  <option value="Moonraker">Moonraker (Klipper)</option>
                  <option value="OctoPrint">OctoPrint</option>
                  <option value="CrealityStock">Creality (firmware stock)</option>
                  <option value="FlashForge">FlashForge (firmware stock)</option>
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
                onClick={handleSave}
                className="px-4 py-2 rounded-lg text-sm font-medium"
                style={{ backgroundColor: 'var(--accent)', color: '#ffffff' }}
              >
                {editingPrinterId ? 'Guardar' : 'Agregar'}
              </button>
            </div>
          </div>
        </div>
      )}
    </>
  )
}

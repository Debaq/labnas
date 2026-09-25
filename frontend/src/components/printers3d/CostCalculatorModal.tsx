import { useState } from 'react'
import { Plus, Trash2, Upload, X, Calculator, DollarSign, FileCode } from 'lucide-react'
import { readGcodeInfo, gramsFromMeters } from '../../lib/gcode'
import type { Printer3DConfig } from '../../types'

// ── Calculadora de costos 3D ──

interface HardwareItem {
  id: string
  name: string
  cost: number | string
  qty: number | string
}

// Piezas de la calculadora de costos. Definidas fuera del modal: si se crean durante
// el render, React las remonta en cada tecla y los inputs pierden el foco.
const calcInputStyle = { backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }
const calcInputClass = "w-full px-3 py-2 rounded-lg text-sm outline-none"

function NumField({ label, hint, value, onChange, unit, placeholder }: {
  label: string; hint?: string; value: number | string; onChange: (v: number | string) => void; unit?: string; placeholder?: string
}) {
  return (
    <div>
      <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>{label}</label>
      <div className="relative">
        <input type="number" min={0} step="any" value={value}
          onChange={(e) => onChange(e.target.value === '' ? '' : parseFloat(e.target.value))}
          placeholder={placeholder || '0'} className={calcInputClass + (unit ? ' pr-10' : '')} style={calcInputStyle} />
        {unit && <span className="absolute right-3 top-1/2 -translate-y-1/2 text-xs pointer-events-none" style={{ color: 'var(--text-secondary)', opacity: 0.6 }}>{unit}</span>}
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

function CalcSectionHeader({ title }: { title: string }) {
  return (
    <div className="flex items-center gap-2 mb-2 mt-1">
      <span className="text-xs font-semibold uppercase tracking-wider" style={{ color: 'var(--accent)' }}>{title}</span>
      <div className="flex-1 h-px" style={{ backgroundColor: 'var(--border)' }} />
    </div>
  )
}

export default function CostCalculatorModal({ printers, onClose }: { printers: Printer3DConfig[]; onClose: () => void }) {
  const [selectedPrinterId, setSelectedPrinterId] = useState<string>('')
  // Material
  const [materialWeight, setMaterialWeight] = useState<number | string>('')
  const [materialPriceKg, setMaterialPriceKg] = useState<number | string>('')
  const [materialDensity, setMaterialDensity] = useState<number | string>('')
  const [stlVolume, setStlVolume] = useState<number | null>(null)
  const [stlFileName, setStlFileName] = useState('')

  function parseSTL(buffer: ArrayBuffer): number {
    const view = new DataView(buffer)
    const numTriangles = view.getUint32(80, true)
    let volume = 0
    for (let i = 0; i < numTriangles; i++) {
      const offset = 84 + i * 50 + 12
      const x1 = view.getFloat32(offset, true), y1 = view.getFloat32(offset + 4, true), z1 = view.getFloat32(offset + 8, true)
      const x2 = view.getFloat32(offset + 12, true), y2 = view.getFloat32(offset + 16, true), z2 = view.getFloat32(offset + 20, true)
      const x3 = view.getFloat32(offset + 24, true), y3 = view.getFloat32(offset + 28, true), z3 = view.getFloat32(offset + 32, true)
      volume += (x1 * (y2 * z3 - y3 * z2) - y1 * (x2 * z3 - x3 * z2) + z1 * (x2 * y3 - x3 * y2)) / 6
    }
    return Math.abs(volume) / 1000
  }

  function handleSTLFile(e: React.ChangeEvent<HTMLInputElement>) {
    const file = e.target.files?.[0]
    if (!file) return
    const reader = new FileReader()
    reader.onload = () => {
      const vol = parseSTL(reader.result as ArrayBuffer)
      setStlVolume(vol)
      setStlFileName(file.name)
      const d = Number(materialDensity)
      if (d > 0) setMaterialWeight(Math.round(vol * d))
    }
    reader.readAsArrayBuffer(file)
    e.target.value = ''
  }

  // Metadatos del gcode (peso y tiempo del slicer)
  const [gcodeNote, setGcodeNote] = useState<string | null>(null)

  async function handleGcodeFile(e: React.ChangeEvent<HTMLInputElement>) {
    const file = e.target.files?.[0]
    e.target.value = ''
    if (!file) return
    const info = await readGcodeInfo(file)
    const parts: string[] = []
    let grams = info.grams
    if (grams === undefined && info.meters !== undefined) {
      const d = Number(materialDensity) || 1.24 // PLA si no hay densidad
      grams = gramsFromMeters(info.meters, d)
      parts.push(`${info.meters.toFixed(2)} m`)
    }
    if (grams !== undefined) {
      setMaterialWeight(Math.round(grams * 10) / 10)
      parts.push(`${grams.toFixed(1)} g`)
    }
    if (info.seconds !== undefined) {
      setPrintHours(Math.round((info.seconds / 3600) * 100) / 100)
      const h = Math.floor(info.seconds / 3600)
      const m = Math.round((info.seconds % 3600) / 60)
      parts.push(h > 0 ? `${h}h ${m}m` : `${m}m`)
    }
    setGcodeNote(parts.length
      ? `${file.name}: ${parts.join(' · ')}${info.slicer ? ` (${info.slicer})` : ''}`
      : `${file.name}: sin metadatos de slicer`)
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

  function handleSelectPrinter(id: string) {
    setSelectedPrinterId(id)
    const p = printers.find(pr => pr.id === id)
    if (p) {
      if (p.power_watts) setPrinterWatts(p.power_watts)
      if (p.electricity_cost_kwh) setKwhPrice(p.electricity_cost_kwh)
    }
  }

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
  const inputStyle = calcInputStyle

  function resetAll() {
    setSelectedPrinterId('')
    setMaterialWeight(''); setMaterialPriceKg(''); setMaterialDensity('')
    setStlVolume(null); setStlFileName('')
    setPrintHours(''); setPrinterWatts(''); setKwhPrice('')
    setMachineCost(''); setMachineLifeHours('')
    setDesignHours(''); setPrepHours(''); setPostProcessHours(''); setHourlyRate('')
    setHardwareItems([])
    setFailRate(''); setMargin(''); setQuantity(1)
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50" onClick={onClose}>
      <div
        className="rounded-xl w-full max-w-3xl mx-4 max-h-[90vh] overflow-y-auto"
        style={{ backgroundColor: 'var(--bg-secondary)', border: '1px solid var(--border)' }}
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center justify-between p-5 pb-0">
          <div className="flex items-center gap-2">
            <Calculator size={18} style={{ color: 'var(--accent)' }} />
            <h3 className="text-sm font-semibold" style={{ color: 'var(--text-primary)' }}>Calculadora de costos</h3>
          </div>
          <div className="flex items-center gap-2">
            {hasAnyCost && (
              <button onClick={resetAll} className="text-xs px-2 py-1 rounded-lg" style={{ color: 'var(--text-secondary)', border: '1px solid var(--border)' }}>
                Limpiar
              </button>
            )}
            <button onClick={onClose} style={{ color: 'var(--text-secondary)' }}><X size={20} /></button>
          </div>
        </div>

        <div className="p-5">
          {/* Selector de impresora */}
          {printers.length > 0 && (
            <div className="mb-4">
              <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Impresora (pre-llena consumo y $/kWh)</label>
              <select
                value={selectedPrinterId}
                onChange={(e) => handleSelectPrinter(e.target.value)}
                className="w-full px-3 py-2 rounded-lg text-sm outline-none cursor-pointer"
                style={inputStyle}
              >
                <option value="">Ingresar manualmente</option>
                {printers.map(p => <option key={p.id} value={p.id}>{p.name}</option>)}
              </select>
            </div>
          )}

          <div className="grid grid-cols-1 lg:grid-cols-[1fr_240px] gap-6">
            <div className="space-y-4">
              {/* Material */}
              <div>
                <CalcSectionHeader title="Material" />
                <div className="flex items-center gap-2 mb-2">
                  <label className="flex items-center gap-2 px-3 py-1.5 rounded-lg text-xs font-medium cursor-pointer"
                    style={{ color: 'var(--accent)', border: '1px dashed var(--border)' }}>
                    <Upload size={12} /> Cargar STL
                    <input type="file" accept=".stl" className="hidden" onChange={handleSTLFile} />
                  </label>
                  <label className="flex items-center gap-2 px-3 py-1.5 rounded-lg text-xs font-medium cursor-pointer"
                    style={{ color: 'var(--accent)', border: '1px dashed var(--border)' }}
                    title="Lee peso y tiempo estimados del slicer (PrusaSlicer, Cura, Orca, Bambu)">
                    <FileCode size={12} /> Cargar GCODE
                    <input type="file" accept=".gcode,.gco,.g,.bgcode" className="hidden" onChange={handleGcodeFile} />
                  </label>
                  {gcodeNote && (
                    <span className="text-[10px]" style={{ color: 'var(--text-secondary)' }}>{gcodeNote}</span>
                  )}
                  {stlVolume !== null && (
                    <span className="text-[10px]" style={{ color: 'var(--text-secondary)' }}>
                      {stlFileName}: {stlVolume.toFixed(2)} cm3
                      {Number(materialDensity) > 0 && ` = ${(stlVolume * Number(materialDensity)).toFixed(0)}g`}
                    </span>
                  )}
                </div>
                <div className="grid grid-cols-3 gap-3">
                  <NumField label="Peso" value={materialWeight} onChange={setMaterialWeight} unit="g" hint="Del slicer o STL" />
                  <NumField label="Precio filamento" value={materialPriceKg} onChange={setMaterialPriceKg} unit="$/kg" />
                  <NumField label="Densidad" value={materialDensity} onChange={(v) => {
                    setMaterialDensity(v)
                    if (stlVolume && Number(v) > 0) setMaterialWeight(Math.round(stlVolume * Number(v)))
                  }} unit="g/cm3" hint="PLA=1.24 ABS=1.04" />
                </div>
              </div>

              {/* Impresora */}
              <div>
                <CalcSectionHeader title="Impresora" />
                <div className="grid grid-cols-3 gap-3">
                  <NumField label="Tiempo" value={printHours} onChange={setPrintHours} unit="h" hint="Del slicer" />
                  <NumField label="Consumo" value={printerWatts} onChange={setPrinterWatts} unit="W" />
                  <NumField label="$/kWh" value={kwhPrice} onChange={setKwhPrice} unit="$/kWh" />
                </div>
                <div className="grid grid-cols-2 gap-3 mt-3">
                  <NumField label="Costo impresora" value={machineCost} onChange={setMachineCost} unit="$" hint="Precio de compra" />
                  <NumField label="Vida util" value={machineLifeHours} onChange={setMachineLifeHours} unit="h" />
                </div>
              </div>

              {/* Mano de obra */}
              <div>
                <CalcSectionHeader title="Mano de obra" />
                <div className="grid grid-cols-2 sm:grid-cols-4 gap-3">
                  <NumField label="Diseno" value={designHours} onChange={setDesignHours} unit="h" />
                  <NumField label="Preparacion" value={prepHours} onChange={setPrepHours} unit="h" />
                  <NumField label="Post-procesado" value={postProcessHours} onChange={setPostProcessHours} unit="h" />
                  <NumField label="Tarifa/h" value={hourlyRate} onChange={setHourlyRate} unit="$/h" />
                </div>
              </div>

              {/* Hardware */}
              <div>
                <CalcSectionHeader title="Hardware adicional" />
                {hardwareItems.map((item) => (
                  <div key={item.id} className="flex items-center gap-2 mb-2">
                    <input type="text" value={item.name} onChange={(e) => updateHardwareItem(item.id, 'name', e.target.value)}
                      placeholder="Descripcion" className="flex-1 px-3 py-1.5 rounded-lg text-sm outline-none" style={inputStyle} />
                    <input type="number" min={0} step="any" value={item.cost}
                      onChange={(e) => updateHardwareItem(item.id, 'cost', e.target.value === '' ? '' : parseFloat(e.target.value))}
                      placeholder="$/u" className="w-20 px-2 py-1.5 rounded-lg text-sm outline-none text-right" style={inputStyle} />
                    <input type="number" min={1} value={item.qty}
                      onChange={(e) => updateHardwareItem(item.id, 'qty', e.target.value === '' ? '' : parseInt(e.target.value))}
                      placeholder="Cant" className="w-16 px-2 py-1.5 rounded-lg text-sm outline-none text-right" style={inputStyle} />
                    <button onClick={() => removeHardwareItem(item.id)} className="p-1 rounded-lg" style={{ color: 'var(--danger)' }}><Trash2 size={14} /></button>
                  </div>
                ))}
                <button onClick={addHardwareItem} className="flex items-center gap-1 text-xs px-3 py-1.5 rounded-lg"
                  style={{ color: 'var(--accent)', border: '1px dashed var(--border)' }}>
                  <Plus size={12} /> Agregar item
                </button>
              </div>

              {/* Ajustes */}
              <div>
                <CalcSectionHeader title="Ajustes" />
                <div className="grid grid-cols-3 gap-3">
                  <NumField label="Tasa de fallo" value={failRate} onChange={setFailRate} unit="%" />
                  <NumField label="Margen" value={margin} onChange={setMargin} unit="%" />
                  <NumField label="Cantidad" value={quantity} onChange={setQuantity} placeholder="1" />
                </div>
              </div>
            </div>

            {/* Resumen */}
            <div>
              <div className="rounded-xl p-4 sticky top-4" style={{ backgroundColor: 'var(--bg-tertiary)', border: '1px solid var(--border)' }}>
                <div className="flex items-center gap-2 mb-3">
                  <DollarSign size={16} style={{ color: 'var(--accent)' }} />
                  <span className="text-xs font-semibold uppercase tracking-wider" style={{ color: 'var(--accent)' }}>Resumen</span>
                </div>
                {!hasAnyCost ? (
                  <p className="text-xs text-center py-4" style={{ color: 'var(--text-secondary)' }}>
                    Completa los campos para calcular
                  </p>
                ) : (
                  <div>
                    <div className="space-y-0.5" style={{ borderBottom: '1px solid var(--border)', paddingBottom: '8px', marginBottom: '8px' }}>
                      <CostLine label="Material" value={materialCost} />
                      <CostLine label="Electricidad" value={electricityCost} />
                      <CostLine label="Depreciacion" value={depreciationCost} />
                      <CostLine label="Mano de obra" value={laborCost} />
                      <CostLine label="Hardware" value={hardwareCost} />
                    </div>
                    <CostLine label="Subtotal" value={subtotal} />
                    {n(failRate) > 0 && <CostLine label={`+ Fallo (${n(failRate)}%)`} value={failAdjusted - subtotal} />}
                    {n(margin) > 0 && <CostLine label={`+ Margen (${n(margin)}%)`} value={withMargin - failAdjusted} />}
                    <div className="mt-3 pt-3" style={{ borderTop: '2px solid var(--accent)' }}>
                      <CostLine label="Costo por unidad" value={totalPerUnit} highlight />
                      {n(quantity) > 1 && <CostLine label={`Total (${n(quantity)} u.)`} value={totalAll} highlight />}
                    </div>
                  </div>
                )}
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  )
}

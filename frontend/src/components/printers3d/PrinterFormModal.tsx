import { useState } from 'react'
import { X } from 'lucide-react'
import { addPrinter3D, updatePrinter3D, reorderPrinter3D } from '../../api'
import type {
  Printer3DConfig,
  DetectPrintersResult,
  Printer3DSection,
} from '../../types'

interface Props {
  printer: Printer3DConfig | null
  prefill: DetectPrintersResult | null
  printers: Printer3DConfig[]
  sections: Printer3DSection[]
  onClose: () => void
  onSaved: () => Promise<void>
}

export default function PrinterFormModal({ printer, prefill, printers, sections, onClose, onSaved }: Props) {
  const [formName, setFormName] = useState(printer?.name ?? (prefill ? (prefill.name || `${prefill.printer_type} @ ${prefill.ip}`) : ''))
  const [formIp, setFormIp] = useState(printer?.ip ?? prefill?.ip ?? '')
  const [formPort, setFormPort] = useState(printer?.port ?? prefill?.port ?? 7125)
  const [formType, setFormType] = useState<'OctoPrint' | 'Moonraker' | 'CrealityStock' | 'FlashForge'>(printer?.printer_type ?? prefill?.printer_type ?? 'Moonraker')
  const [formApiKey, setFormApiKey] = useState(printer?.api_key || '')
  const [formCameraUrl, setFormCameraUrl] = useState(printer?.camera_url || '')
  const [formSectionId, setFormSectionId] = useState<string | null>(printer?.section_id || null)
  const [formPowerWatts, setFormPowerWatts] = useState<number | string>(printer?.power_watts ?? '')
  const [formElectricityCost, setFormElectricityCost] = useState<number | string>(printer?.electricity_cost_kwh ?? '')
  const editingPrinterId = printer?.id ?? null

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
          section_id: formSectionId,
          power_watts: Number(formPowerWatts) || null,
          electricity_cost_kwh: Number(formElectricityCost) || null,
        })
      } else {
        const created = await addPrinter3D({
          name: formName,
          ip: formIp,
          port: formPort,
          printer_type: formType,
          api_key: formApiKey || null,
          camera_url: formCameraUrl || null,
        })
        // Asignar seccion si se selecciono
        if (formSectionId) {
          const targetPrinters = printers.filter(p => (p.section_id || null) === formSectionId)
          const maxOrder = targetPrinters.length > 0 ? Math.max(...targetPrinters.map(p => p.order)) + 1 : 0
          await reorderPrinter3D(created.id, formSectionId, maxOrder)
        }
      }
      onClose()
      await onSaved()
    } catch (err) {
      console.error('Error guardando impresora:', err)
    }
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
      <div
        className="rounded-xl p-6 w-full max-w-md mx-4 max-h-[90vh] overflow-y-auto"
        style={{ backgroundColor: 'var(--bg-secondary)', border: '1px solid var(--border)' }}
      >
        <div className="flex items-center justify-between mb-6">
          <h3 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>
            {editingPrinterId ? 'Editar Impresora 3D' : 'Agregar Impresora 3D'}
          </h3>
          <button onClick={() => onClose()} style={{ color: 'var(--text-secondary)' }}>
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
          <div className="grid grid-cols-2 gap-3">
            <div>
              <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Consumo (W)</label>
              <input
                type="number"
                min={0}
                step="any"
                value={formPowerWatts}
                onChange={(e) => setFormPowerWatts(e.target.value === '' ? '' : parseFloat(e.target.value))}
                placeholder="Ej: 350"
                className="w-full px-3 py-2 rounded-lg text-sm outline-none font-mono"
                style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
              />
            </div>
            <div>
              <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>$/kWh</label>
              <input
                type="number"
                min={0}
                step="any"
                value={formElectricityCost}
                onChange={(e) => setFormElectricityCost(e.target.value === '' ? '' : parseFloat(e.target.value))}
                placeholder="Ej: 0.15"
                className="w-full px-3 py-2 rounded-lg text-sm outline-none font-mono"
                style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
              />
            </div>
          </div>
          {sections.length > 0 && (
            <div>
              <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Seccion (opcional)</label>
              <select
                value={formSectionId || ''}
                onChange={(e) => setFormSectionId(e.target.value || null)}
                className="w-full px-3 py-2 rounded-lg text-sm outline-none cursor-pointer"
                style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
              >
                <option value="">Sin seccion</option>
                {sections.map(s => <option key={s.id} value={s.id}>{s.name}</option>)}
              </select>
            </div>
          )}
        </div>

        <div className="flex items-center justify-end gap-3 mt-6">
          <button
            onClick={() => onClose()}
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
  )
}

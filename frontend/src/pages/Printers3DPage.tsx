import { useEffect, useState, useCallback } from 'react'
import { useEvent, useEventsConnected } from '../events/useEvents'
import { Box } from 'lucide-react'
import {
  fetchPrinters3D,
  deletePrinter3D,
  fetchPrinter3DStatus,
  fetchPrinter3DSections,
} from '../api'
import type {
  Printer3DConfig,
  Printer3DStatus,
  DetectPrintersResult,
  Printer3DSection,
} from '../types'
import CostCalculatorModal from '../components/printers3d/CostCalculatorModal'
import PrintQueueModal from '../components/printers3d/PrintQueueModal'
import TimelapseModal from '../components/printers3d/TimelapseModal'
import PrinterSidebar from '../components/printers3d/PrinterSidebar'
import PrinterDetail from '../components/printers3d/PrinterDetail'
import PrinterFormModal from '../components/printers3d/PrinterFormModal'

export default function Printers3DPage() {
  const [printers, setPrinters] = useState<Printer3DConfig[]>([])
  const [statuses, setStatuses] = useState<Record<string, Printer3DStatus>>({})
  const [loading, setLoading] = useState(true)
  const [selectedPrinterId, setSelectedPrinterId] = useState<string | null>(null)
  const [sections, setSections] = useState<Printer3DSection[]>([])
  const [formModal, setFormModal] = useState<{ printer: Printer3DConfig | null; prefill: DetectPrintersResult | null } | null>(null)
  const [showCostCalc, setShowCostCalc] = useState(false)
  const [showQueue, setShowQueue] = useState(false)
  const [showTimelapse, setShowTimelapse] = useState(false)

  const loadPrinters = useCallback(async () => {
    try {
      const [printersData, sectionsData] = await Promise.all([
        fetchPrinters3D(),
        fetchPrinter3DSections(),
      ])
      setPrinters(printersData)
      setSections(sectionsData)
    } catch {
      setPrinters([])
      setSections([])
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => {
    loadPrinters()
  }, [loadPrinters])

  // Estados: en vivo por el bus (el servidor consulta una vez para todas las pestañas);
  // consulta directa cada 5 s solo sin conexion
  const liveEvents = useEventsConnected()
  const fetchAllStatuses = useCallback(async () => {
    if (printers.length === 0) return
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
  }, [printers])

  useEvent<Record<string, Printer3DStatus>>(
    'printers3d.status',
    (map) => setStatuses((prev) => ({ ...prev, ...map })),
    fetchAllStatuses,
  )

  useEffect(() => { fetchAllStatuses() }, [fetchAllStatuses])

  useEffect(() => {
    if (liveEvents || printers.length === 0) return
    const interval = setInterval(fetchAllStatuses, 5000)
    return () => clearInterval(interval)
  }, [liveEvents, printers, fetchAllStatuses])

  async function handleDelete(id: string) {
    if (!confirm('Eliminar esta impresora?')) return
    try {
      await deletePrinter3D(id)
      await loadPrinters()
    } catch (err) {
      console.error('Error eliminando impresora:', err)
    }
  }

  const selectedPrinter = printers.find(p => p.id === selectedPrinterId) || null

  return (
    <>
      <div className="flex gap-0 h-full -m-8" style={{ minHeight: 'calc(100vh - 130px)' }}>
        <PrinterSidebar
          printers={printers}
          sections={sections}
          statuses={statuses}
          loading={loading}
          selectedPrinterId={selectedPrinterId}
          onSelect={setSelectedPrinterId}
          onReload={loadPrinters}
          onAdd={() => setFormModal({ printer: null, prefill: null })}
          onFillDetected={(d) => setFormModal({ printer: null, prefill: d })}
          onShowCostCalc={() => setShowCostCalc(true)}
          onShowQueue={() => setShowQueue(true)}
          onShowTimelapse={() => setShowTimelapse(true)}
        />

        {/* Right panel: selected printer or empty state */}
        <div className="flex-1 overflow-y-auto p-6">
          {!selectedPrinter ? (
            <div className="flex flex-col items-center justify-center h-full">
              <Box size={48} className="mb-3" style={{ color: 'var(--text-secondary)', opacity: 0.3 }} />
              <p className="text-sm" style={{ color: 'var(--text-secondary)' }}>
                {printers.length === 0 ? 'Agrega una impresora para empezar' : 'Selecciona una impresora'}
              </p>
            </div>
          ) : (
            <PrinterDetail
              printer={selectedPrinter}
              status={statuses[selectedPrinter.id]}
              onEdit={(printer) => setFormModal({ printer, prefill: null })}
              onDelete={handleDelete}
            />
          )}
        </div>
      </div>

      {formModal && (
        <PrinterFormModal
          printer={formModal.printer}
          prefill={formModal.prefill}
          printers={printers}
          sections={sections}
          onClose={() => setFormModal(null)}
          onSaved={loadPrinters}
        />
      )}

      {showCostCalc && <CostCalculatorModal printers={printers} onClose={() => setShowCostCalc(false)} />}
      {showQueue && <PrintQueueModal printers={printers} onClose={() => setShowQueue(false)} />}
      {showTimelapse && <TimelapseModal printers={printers} onClose={() => setShowTimelapse(false)} />}
    </>
  )
}

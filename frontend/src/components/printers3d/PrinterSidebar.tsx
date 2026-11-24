import { useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { Box, ArrowLeft, Plus, Trash2, Loader2, Search, X, Home, ChevronDown, ArrowUp, ArrowDown, Calculator, Pencil, FolderPlus, ChevronRight, Check, ListOrdered, Film } from 'lucide-react'
import {
  detectPrinters3D,
  addPrinter3DSection,
  updatePrinter3DSection,
  deletePrinter3DSection,
  reorderPrinter3D,
  reorderPrinter3DSections,
  testHome3D,
} from '../../api'
import type {
  Printer3DConfig,
  Printer3DStatus,
  DetectPrintersResult,
  Printer3DSection,
} from '../../types'
import PrinterListItem from './PrinterListItem'

interface Props {
  printers: Printer3DConfig[]
  sections: Printer3DSection[]
  statuses: Record<string, Printer3DStatus>
  loading: boolean
  selectedPrinterId: string | null
  onSelect: (id: string) => void
  onReload: () => Promise<void>
  onAdd: () => void
  onFillDetected: (d: DetectPrintersResult) => void
  onShowCostCalc: () => void
  onShowQueue: () => void
  onShowTimelapse: () => void
}

export default function PrinterSidebar({
  printers, sections, statuses, loading, selectedPrinterId,
  onSelect, onReload, onAdd, onFillDetected, onShowCostCalc, onShowQueue, onShowTimelapse,
}: Props) {
  const navigate = useNavigate()
  const [detecting, setDetecting] = useState(false)
  const [detected, setDetected] = useState<DetectPrintersResult[]>([])
  const [collapsedSections, setCollapsedSections] = useState<Set<string>>(new Set())
  const [editingSectionId, setEditingSectionId] = useState<string | null>(null)
  const [editingSectionName, setEditingSectionName] = useState('')
  const [testingHome, setTestingHome] = useState<string | null>(null) // ip:port key

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
    setDetected([])
    onFillDetected(d)
  }

  // ── Secciones ──

  async function handleAddSection() {
    const name = prompt('Nombre de la seccion:')
    if (!name?.trim()) return
    try {
      await addPrinter3DSection(name.trim())
      await onReload()
    } catch (err) {
      console.error('Error creando seccion:', err)
    }
  }

  async function handleSaveSection(id: string) {
    if (!editingSectionName.trim()) return
    try {
      await updatePrinter3DSection(id, { name: editingSectionName.trim() })
      setEditingSectionId(null)
      await onReload()
    } catch (err) {
      console.error('Error actualizando seccion:', err)
    }
  }

  async function handleDeleteSection(id: string) {
    if (!confirm('Eliminar esta seccion? Las impresoras se moveran a "Sin seccion".')) return
    try {
      await deletePrinter3DSection(id)
      await onReload()
    } catch (err) {
      console.error('Error eliminando seccion:', err)
    }
  }

  function toggleSection(id: string) {
    setCollapsedSections(prev => {
      const next = new Set(prev)
      if (next.has(id)) next.delete(id)
      else next.add(id)
      return next
    })
  }

  async function movePrinter(printerId: string, direction: 'up' | 'down') {
    const printer = printers.find(p => p.id === printerId)
    if (!printer) return
    const sectionId = printer.section_id || null
    const sectionPrinters = printers
      .filter(p => (p.section_id || null) === sectionId)
      .sort((a, b) => a.order - b.order)
    const idx = sectionPrinters.findIndex(p => p.id === printerId)
    if (direction === 'up' && idx <= 0) return
    if (direction === 'down' && idx >= sectionPrinters.length - 1) return
    const swapIdx = direction === 'up' ? idx - 1 : idx + 1
    const other = sectionPrinters[swapIdx]
    try {
      await Promise.all([
        reorderPrinter3D(printerId, sectionId, other.order),
        reorderPrinter3D(other.id, sectionId, printer.order),
      ])
      await onReload()
    } catch (err) {
      console.error('Error reordenando:', err)
    }
  }

  async function movePrinterToSection(printerId: string, targetSectionId: string | null) {
    const targetPrinters = printers.filter(p => (p.section_id || null) === targetSectionId)
    const maxOrder = targetPrinters.length > 0 ? Math.max(...targetPrinters.map(p => p.order)) + 1 : 0
    try {
      await reorderPrinter3D(printerId, targetSectionId, maxOrder)
      await onReload()
    } catch (err) {
      console.error('Error moviendo impresora:', err)
    }
  }

  async function moveSectionOrder(sectionId: string, direction: 'up' | 'down') {
    const sorted = [...sections].sort((a, b) => a.order - b.order)
    const idx = sorted.findIndex(s => s.id === sectionId)
    if (direction === 'up' && idx <= 0) return
    if (direction === 'down' && idx >= sorted.length - 1) return
    const swapIdx = direction === 'up' ? idx - 1 : idx + 1
    ;[sorted[idx], sorted[swapIdx]] = [sorted[swapIdx], sorted[idx]]
    try {
      await reorderPrinter3DSections(sorted.map(s => s.id))
      await onReload()
    } catch (err) {
      console.error('Error reordenando secciones:', err)
    }
  }

  // Helper: obtener impresoras ordenadas por seccion
  function getPrintersForSection(sectionId: string | null): Printer3DConfig[] {
    return printers
      .filter(p => (p.section_id || null) === sectionId)
      .sort((a, b) => a.order - b.order)
  }

  function renderPrinterItem(printer: Printer3DConfig, sectionId: string | null) {
    const sectionPrinters = getPrintersForSection(sectionId)
    return (
      <PrinterListItem
        key={printer.id}
        printer={printer}
        status={statuses[printer.id]}
        isSelected={selectedPrinterId === printer.id}
        sectionId={sectionId}
        sections={sections}
        index={sectionPrinters.findIndex(p => p.id === printer.id)}
        count={sectionPrinters.length}
        onSelect={() => onSelect(printer.id)}
        onMove={(direction) => movePrinter(printer.id, direction)}
        onMoveToSection={(target) => movePrinterToSection(printer.id, target)}
      />
    )
  }

  return (
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
            className="flex items-center justify-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium"
            style={{ color: 'var(--text-primary)', border: '1px solid var(--border)' }}
          >
            {detecting ? <Loader2 size={12} className="animate-spin" /> : <Search size={12} />}
            Buscar
          </button>
          <button
            onClick={onAdd}
            className="flex items-center justify-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium"
            style={{ backgroundColor: 'var(--accent)', color: '#fff' }}
          >
            <Plus size={12} /> Agregar
          </button>
          <button
            onClick={handleAddSection}
            className="flex items-center justify-center gap-1.5 px-2 py-1.5 rounded-lg text-xs font-medium"
            style={{ color: 'var(--text-secondary)', border: '1px solid var(--border)' }}
            title="Nueva seccion"
          >
            <FolderPlus size={12} />
          </button>
        </div>
        <button
          onClick={() => onShowCostCalc()}
          className="w-full flex items-center justify-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium"
          style={{ color: 'var(--text-secondary)', border: '1px solid var(--border)' }}
        >
          <Calculator size={12} /> Calculadora de costos
        </button>
        <button
          onClick={() => onShowQueue()}
          className="w-full flex items-center justify-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium"
          style={{ color: 'var(--text-secondary)', border: '1px solid var(--border)' }}
        >
          <ListOrdered size={12} /> Cola de impresion
        </button>
        <button
          onClick={() => onShowTimelapse()}
          className="w-full flex items-center justify-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium"
          style={{ color: 'var(--text-secondary)', border: '1px solid var(--border)' }}
        >
          <Film size={12} /> Timelapse
        </button>
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
            {newDetected.map((d, i) => {
              const homeKey = `${d.ip}:${d.port}`
              const isHoming = testingHome === homeKey
              return (
                <div key={i} className="flex items-center gap-1 rounded-lg text-xs"
                  style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--border)' }}>
                  <button onClick={() => fillFromDetected(d)} className="flex-1 text-left px-2 py-1.5 hover:opacity-80">
                    <span className="font-mono">{d.ip}</span>
                    <span className="ml-1.5 text-[10px]" style={{ color: 'var(--accent)' }}>{d.printer_type}</span>
                    {d.name && <span className="ml-1 text-[10px]" style={{ color: 'var(--text-secondary)' }}>{d.name}</span>}
                  </button>
                  <button
                    onClick={async (e) => {
                      e.stopPropagation()
                      setTestingHome(homeKey)
                      try {
                        await testHome3D(d.ip, d.port, d.printer_type)
                      } catch (err) {
                        console.error('Error test home:', err)
                      } finally {
                        setTestingHome(null)
                      }
                    }}
                    disabled={isHoming}
                    className="px-2 py-1.5 rounded-r-lg hover:opacity-80 shrink-0"
                    style={{ color: 'var(--accent)', borderLeft: '1px solid var(--border)' }}
                    title="Enviar Home para identificar"
                  >
                    {isHoming ? <Loader2 size={12} className="animate-spin" /> : <Home size={12} />}
                  </button>
                </div>
              )
            })}
          </div>
        )
      })()}

      {/* Printer list grouped by sections */}
      <div className="flex-1 overflow-y-auto">
        {loading ? (
          <div className="flex justify-center py-8"><Loader2 size={20} className="animate-spin" style={{ color: 'var(--accent)' }} /></div>
        ) : printers.length === 0 && sections.length === 0 ? (
          <div className="text-center py-8 px-4">
            <Box size={28} className="mx-auto mb-2" style={{ color: 'var(--text-secondary)', opacity: 0.4 }} />
            <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>Sin impresoras</p>
          </div>
        ) : (() => {
          const sortedSections = [...sections].sort((a, b) => a.order - b.order)
          const unsectioned = getPrintersForSection(null)
          const allGroups: { section: Printer3DSection | null; printers: Printer3DConfig[] }[] = [
            ...sortedSections.map(s => ({ section: s, printers: getPrintersForSection(s.id) })),
            ...(unsectioned.length > 0 || sortedSections.length > 0 ? [{ section: null, printers: unsectioned }] : []),
          ]

          return (
            <>
              {allGroups.map(({ section, printers: groupPrinters }) => {
                if (section === null && sortedSections.length === 0) {
                  // Sin secciones: lista plana
                  return groupPrinters.map(p => renderPrinterItem(p, null))
                }
                const isCollapsed = section ? collapsedSections.has(section.id) : collapsedSections.has('__unsectioned__')
                const collapseKey = section?.id || '__unsectioned__'
                return (
                  <div key={collapseKey}>
                    {/* Section header */}
                    <div
                      className="flex items-center gap-1 px-2 py-1.5 group/sec"
                      style={{ backgroundColor: 'var(--bg-tertiary)', borderBottom: '1px solid var(--border)' }}
                    >
                      <button
                        onClick={() => toggleSection(collapseKey)}
                        className="flex items-center gap-1 flex-1 min-w-0"
                      >
                        {isCollapsed ? <ChevronRight size={12} style={{ color: 'var(--text-secondary)' }} /> : <ChevronDown size={12} style={{ color: 'var(--text-secondary)' }} />}
                        {editingSectionId === section?.id ? (
                          <input
                            autoFocus
                            value={editingSectionName}
                            onChange={(e) => setEditingSectionName(e.target.value)}
                            onKeyDown={(e) => { if (e.key === 'Enter') handleSaveSection(section!.id); if (e.key === 'Escape') setEditingSectionId(null) }}
                            onClick={(e) => e.stopPropagation()}
                            className="flex-1 px-1 py-0 text-[11px] font-semibold uppercase tracking-wider rounded outline-none"
                            style={{ backgroundColor: 'var(--input-bg)', color: 'var(--accent)', border: '1px solid var(--accent)', maxWidth: 130 }}
                          />
                        ) : (
                          <span className="text-[11px] font-semibold uppercase tracking-wider truncate" style={{ color: 'var(--accent)' }}>
                            {section?.name || 'Sin seccion'}
                          </span>
                        )}
                        <span className="text-[10px] shrink-0" style={{ color: 'var(--text-secondary)' }}>({groupPrinters.length})</span>
                      </button>
                      {section && (
                        <div className="flex items-center gap-0.5 opacity-0 group-hover/sec:opacity-100 transition-opacity">
                          {editingSectionId === section.id ? (
                            <button onClick={() => handleSaveSection(section.id)} className="p-0.5 rounded hover:opacity-80" style={{ color: 'var(--success)' }}>
                              <Check size={11} />
                            </button>
                          ) : (
                            <button onClick={(e) => { e.stopPropagation(); setEditingSectionId(section.id); setEditingSectionName(section.name) }}
                              className="p-0.5 rounded hover:opacity-80" style={{ color: 'var(--text-secondary)' }} title="Renombrar">
                              <Pencil size={11} />
                            </button>
                          )}
                          <button onClick={(e) => { e.stopPropagation(); moveSectionOrder(section.id, 'up') }}
                            className="p-0.5 rounded hover:opacity-80" style={{ color: 'var(--text-secondary)' }} title="Subir seccion">
                            <ArrowUp size={11} />
                          </button>
                          <button onClick={(e) => { e.stopPropagation(); moveSectionOrder(section.id, 'down') }}
                            className="p-0.5 rounded hover:opacity-80" style={{ color: 'var(--text-secondary)' }} title="Bajar seccion">
                            <ArrowDown size={11} />
                          </button>
                          <button onClick={(e) => { e.stopPropagation(); handleDeleteSection(section.id) }}
                            className="p-0.5 rounded hover:opacity-80" style={{ color: 'var(--danger)' }} title="Eliminar seccion">
                            <Trash2 size={11} />
                          </button>
                        </div>
                      )}
                    </div>
                    {/* Printers in section */}
                    {!isCollapsed && groupPrinters.map(p => renderPrinterItem(p, section?.id || null))}
                  </div>
                )
              })}
            </>
          )
        })()}
      </div>
    </div>
  )
}

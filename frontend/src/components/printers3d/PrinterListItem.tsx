import { Thermometer, Move, ArrowUp, ArrowDown } from 'lucide-react'
import type { Printer3DConfig, Printer3DStatus, Printer3DSection } from '../../types'
import { formatTime } from './utils'

interface Props {
  printer: Printer3DConfig
  status: Printer3DStatus | undefined
  isSelected: boolean
  sectionId: string | null
  sections: Printer3DSection[]
  index: number
  count: number
  onSelect: () => void
  onMove: (direction: 'up' | 'down') => void
  onMoveToSection: (sectionId: string | null) => void
}

export default function PrinterListItem({ printer, status, isSelected, sectionId, sections, index, count, onSelect, onMove, onMoveToSection }: Props) {
  const isOnline = status?.online ?? false
  const temps = status?.temperatures
  const job = status?.current_job
  const isPrinting = job && job.state.toLowerCase().includes('print')
  const isPaused = job && job.state.toLowerCase().includes('paus')
  return (
    <div
      key={printer.id}
      className="group relative"
      style={{ borderBottom: '1px solid var(--border)' }}
    >
      <button
        onClick={onSelect}
        className="w-full text-left p-3 pr-8 transition-all duration-150 hover:opacity-90"
        style={{
          backgroundColor: isSelected ? 'var(--accent-alpha)' : 'transparent',
          borderLeft: isSelected ? '3px solid var(--accent)' : '3px solid transparent',
        }}
      >
        <div className="flex items-center gap-2 mb-1">
          <span className="w-2 h-2 rounded-full shrink-0" style={{ backgroundColor: isOnline ? 'var(--success)' : 'var(--danger)' }} />
          <span className="text-sm font-medium truncate" style={{ color: 'var(--text-primary)' }}>{printer.name}</span>
        </div>
        {isOnline && temps && (
          <div className="flex items-center gap-2 ml-4 text-[10px] mb-0.5" style={{ color: 'var(--text-secondary)' }}>
            <span title="Hotend">
              <Thermometer size={9} className="inline mr-0.5" style={{ color: 'var(--danger)' }} />
              {temps.hotend_actual.toFixed(0)}°
            </span>
            <span title="Cama">
              <Thermometer size={9} className="inline mr-0.5" style={{ color: 'var(--warning)' }} />
              {temps.bed_actual.toFixed(0)}°
            </span>
          </div>
        )}
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
      {/* Move controls on hover */}
      <div className="absolute right-1 top-1 flex flex-col gap-0.5 opacity-0 group-hover:opacity-100 transition-opacity">
        {index > 0 && (
          <button onClick={(e) => { e.stopPropagation(); onMove('up') }}
            className="p-0.5 rounded hover:opacity-80" style={{ color: 'var(--text-secondary)' }} title="Subir">
            <ArrowUp size={11} />
          </button>
        )}
        {index < count - 1 && (
          <button onClick={(e) => { e.stopPropagation(); onMove('down') }}
            className="p-0.5 rounded hover:opacity-80" style={{ color: 'var(--text-secondary)' }} title="Bajar">
            <ArrowDown size={11} />
          </button>
        )}
        {/* Move to section dropdown */}
        {(sections.length > 0 || sectionId !== null) && (
          <div className="relative group/move">
            <button className="p-0.5 rounded hover:opacity-80" style={{ color: 'var(--text-secondary)' }} title="Mover a seccion">
              <Move size={11} />
            </button>
            <div className="absolute right-full top-0 mr-1 hidden group-hover/move:block z-50"
              style={{ minWidth: 120 }}>
              <div className="rounded-lg py-1 shadow-lg"
                style={{ backgroundColor: 'var(--bg-secondary)', border: '1px solid var(--border)' }}>
                {sectionId !== null && (
                  <button
                    onClick={(e) => { e.stopPropagation(); onMoveToSection(null) }}
                    className="w-full text-left px-3 py-1.5 text-[10px] hover:opacity-80"
                    style={{ color: 'var(--text-primary)' }}
                  >
                    Sin seccion
                  </button>
                )}
                {sections.filter(s => s.id !== sectionId).map(s => (
                  <button
                    key={s.id}
                    onClick={(e) => { e.stopPropagation(); onMoveToSection(s.id) }}
                    className="w-full text-left px-3 py-1.5 text-[10px] hover:opacity-80"
                    style={{ color: 'var(--text-primary)' }}
                  >
                    {s.name}
                  </button>
                ))}
              </div>
            </div>
          </div>
        )}
      </div>
    </div>
  )
}

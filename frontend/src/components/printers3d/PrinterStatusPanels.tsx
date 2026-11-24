import { Thermometer, FileText, Play, Pause, Square } from 'lucide-react'
import type { Printer3DStatus } from '../../types'
import { formatTime } from './utils'

type Temps = NonNullable<Printer3DStatus['temperatures']>
type Job = NonNullable<Printer3DStatus['current_job']>

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

export function TemperaturesPanel({ temps }: { temps: Temps }) {
  return (
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
  )
}

export function JobPanel({ job, onControl }: { job: Job; onControl: (command: 'start' | 'pause' | 'resume' | 'cancel') => void }) {
  const isPrinting = job.state.toLowerCase().includes('printing') || job.state.toLowerCase() === 'printing'
  const isPaused = job.state.toLowerCase().includes('paus')

  return (
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
            onClick={() => onControl('start')}
            className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium hover:opacity-90"
            style={{ backgroundColor: 'var(--success)', color: '#ffffff' }}
            title="Iniciar"
          >
            <Play size={12} /> Iniciar
          </button>
        )}
        {isPrinting && (
          <button
            onClick={() => onControl('pause')}
            className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium hover:opacity-90"
            style={{ backgroundColor: 'var(--warning)', color: '#ffffff' }}
            title="Pausar"
          >
            <Pause size={12} /> Pausar
          </button>
        )}
        {isPaused && (
          <button
            onClick={() => onControl('resume')}
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
                onControl('cancel')
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
  )
}

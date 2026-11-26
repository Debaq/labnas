import type { ReactNode } from 'react'
import { Link } from 'react-router-dom'
import { Check, Pencil, RotateCcw, Trash2 } from 'lucide-react'
import { categoryInfo, countdownLabel, levelColor, longDate, type Deadline } from '../../api'

interface Props {
  d: Deadline
  /** Mostrar botones (solo contadores sueltos que la persona puede cambiar) */
  editable?: boolean
  compact?: boolean
  onEdit?: (d: Deadline) => void
  onDone?: (d: Deadline, done: boolean) => void
  onDelete?: (d: Deadline) => void
  /** Nivel de acceso (insignia o boton), solo contadores sueltos */
  access?: ReactNode
}

/** Tarjeta con la cuenta regresiva de un plazo; el color cambia al acercarse la fecha */
export default function DeadlineCard({ d, editable, compact, onEdit, onDone, onDelete, access }: Props) {
  const color = levelColor(d.level)
  const c = countdownLabel(d)
  const cat = categoryInfo(d.category)
  const btn = 'p-1.5 rounded-md transition-colors hover:bg-[var(--bg-tertiary,var(--bg-secondary))]'

  return (
    <div
      className="flex items-stretch rounded-xl overflow-hidden"
      style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)', borderLeft: `4px solid ${color}` }}
    >
      <div
        className={`flex flex-col items-center justify-center shrink-0 ${compact ? 'w-16 py-2' : 'w-20 py-3'}`}
        style={{ backgroundColor: `color-mix(in srgb, ${color} 12%, transparent)`, color }}
      >
        <span className={`font-bold leading-none ${compact ? 'text-xl' : 'text-3xl'}`}>{c.big}</span>
        <span className="text-[10px] mt-1 text-center leading-tight px-1">{c.small}</span>
      </div>
      <div className={`flex-1 min-w-0 ${compact ? 'px-3 py-2' : 'px-4 py-3'}`}>
        <div className="flex items-start justify-between gap-2">
          <div className="min-w-0">
            <div className="text-sm font-medium truncate" style={{ color: 'var(--text-primary)' }} title={d.title}>
              <span aria-hidden className="mr-1">{cat.emoji}</span>{d.title}
            </div>
            <div className="text-xs truncate" style={{ color: 'var(--text-secondary)' }}>
              {longDate(d.due_date)}
              {d.project && <> · {d.portfolio_id ? <Link to="/portfolio" className="hover:underline">{d.project}</Link> : d.project}</>}
              {!compact && (d.source === 'proyecto' || d.source === 'hito') && ' · desde el Portafolio'}
              {!compact && d.source === 'induccion' && ' · desde Máquinas'}
            </div>
            {!compact && d.notes && (
              <div className="text-xs mt-1 line-clamp-2" style={{ color: 'var(--text-secondary)' }}>{d.notes}</div>
            )}
          </div>
          {!compact && access && !editable && <div className="shrink-0">{access}</div>}
          {editable && !compact && d.id && (
            <div className="flex items-center gap-0.5 shrink-0" style={{ color: 'var(--text-secondary)' }}>
              {access}
              {d.done_at ? (
                <button className={btn} title="Reabrir" onClick={() => onDone?.(d, false)}><RotateCcw size={14} /></button>
              ) : (
                <>
                  <button className={btn} title="Marcar cumplido" onClick={() => onDone?.(d, true)} style={{ color: 'var(--success)' }}><Check size={15} /></button>
                  <button className={btn} title="Editar" onClick={() => onEdit?.(d)}><Pencil size={14} /></button>
                </>
              )}
              <button className={btn} title="Eliminar" onClick={() => onDelete?.(d)} style={{ color: 'var(--danger)' }}><Trash2 size={14} /></button>
            </div>
          )}
        </div>
      </div>
    </div>
  )
}

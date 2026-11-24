import { X } from 'lucide-react'
import type { ScheduleModalState } from './useScheduleModal'

export default function ScheduleModal({ schedule, loadData }: { schedule: ScheduleModalState; loadData: () => Promise<void> }) {
  const { showScheduleModal, setShowScheduleModal, scheduleDate, setScheduleDate, scheduleTime, setScheduleTime, scheduleMissing } = schedule
  if (!showScheduleModal) return null
  return (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
          <div className="rounded-xl p-6 w-full max-w-sm mx-4" style={{ backgroundColor: 'var(--bg-secondary)', border: '1px solid var(--border)' }}>
            <div className="flex items-center justify-between mb-4">
              <h3 className="text-base font-semibold" style={{ color: 'var(--text-primary)' }}>Agendar Tarea</h3>
              <button onClick={() => setShowScheduleModal(false)} style={{ color: 'var(--text-secondary)' }}><X size={18} /></button>
            </div>
            <p className="text-xs mb-4" style={{ color: 'var(--text-secondary)' }}>
              Completa {scheduleMissing === 'both' ? 'la fecha y hora' : scheduleMissing === 'date' ? 'la fecha' : 'la hora'} para crear la actividad en el calendario.
            </p>
            <div className="space-y-3">
              {(scheduleMissing === 'both' || scheduleMissing === 'date') && (
                <div>
                  <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Fecha</label>
                  <input
                    type="date"
                    value={scheduleDate}
                    onChange={(e) => setScheduleDate(e.target.value)}
                    className="w-full px-3 py-2 rounded-lg text-sm outline-none"
                    style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
                    autoFocus
                  />
                </div>
              )}
              {(scheduleMissing === 'both' || scheduleMissing === 'time') && (
                <div>
                  <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Hora</label>
                  <input
                    type="time"
                    value={scheduleTime}
                    onChange={(e) => setScheduleTime(e.target.value)}
                    className="w-full px-3 py-2 rounded-lg text-sm outline-none"
                    style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
                  />
                </div>
              )}
            </div>
            <div className="flex justify-end gap-3 mt-5">
              <button
                onClick={() => setShowScheduleModal(false)}
                className="px-4 py-2 rounded-lg text-sm font-medium"
                style={{ color: 'var(--text-secondary)', border: '1px solid var(--border)' }}
              >Cancelar</button>
              <button
                onClick={() => schedule.confirm(loadData)}
                disabled={!scheduleDate || !scheduleTime}
                className="px-4 py-2 rounded-lg text-sm font-medium"
                style={{ backgroundColor: 'var(--accent)', color: '#ffffff', opacity: scheduleDate && scheduleTime ? 1 : 0.5 }}
              >Agendar</button>
            </div>
          </div>
        </div>
  )
}

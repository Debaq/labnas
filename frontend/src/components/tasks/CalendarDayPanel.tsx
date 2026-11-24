import { Trash2, CheckCircle2, XCircle, ClipboardList, Calendar, Pencil, MapPin, BellOff } from 'lucide-react'
import { acceptEvent, declineEvent, deleteEvent } from '../../api'
import type { Task, CalendarEvent, EventCategory } from '../../types'
import { recurrenceLabel } from './utils'

interface Props {
  selectedDay: string | null
  selectedEvents: CalendarEvent[]
  selectedTasks: Task[]
  catMap: Map<string, EventCategory>
  getCatColor: (ev: CalendarEvent) => string | undefined
  username?: string
  loadData: () => Promise<void>
  onEdit: (ev: CalendarEvent) => void
}

export default function CalendarDayPanel({ selectedDay, selectedEvents, selectedTasks, catMap, getCatColor, username, loadData, onEdit }: Props) {
  return (
            <div className="space-y-3">
              {selectedDay ? (
                <>
                  <div className="rounded-xl p-4" style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}>
                    <h3 className="text-sm font-semibold mb-1" style={{ color: 'var(--text-primary)' }}>
                      {new Date(selectedDay + 'T12:00:00').toLocaleDateString('es', { weekday: 'long', day: 'numeric', month: 'long' })}
                    </h3>
                    <span className="text-[10px]" style={{ color: 'var(--text-secondary)' }}>
                      {selectedEvents.length} evento{selectedEvents.length !== 1 ? 's' : ''}, {selectedTasks.length} tarea{selectedTasks.length !== 1 ? 's' : ''}
                    </span>
                  </div>

                  {selectedEvents.length === 0 && selectedTasks.length === 0 && (
                    <div className="rounded-xl p-6 text-center" style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}>
                      <Calendar size={24} className="mx-auto mb-2" style={{ color: 'var(--text-secondary)', opacity: 0.4 }} />
                      <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>Sin eventos este dia</p>
                    </div>
                  )}

                  {selectedEvents.map(ev => {
                    const isPast = `${ev.date} ${ev.time}` < new Date().toISOString().slice(0, 16).replace('T', ' ')
                    const catColor = getCatColor(ev)
                    const cat = ev.category ? catMap.get(ev.category) : undefined
                    return (
                      <div key={ev.id} className="rounded-xl p-4" style={{ backgroundColor: 'var(--card-bg)', borderLeft: `4px solid ${catColor || 'var(--accent)'}`, borderTop: '1px solid var(--card-border)', borderRight: '1px solid var(--card-border)', borderBottom: '1px solid var(--card-border)', opacity: isPast ? 0.5 : 1 }}>
                        <div className="flex items-start justify-between gap-2">
                          <div className="min-w-0">
                            <div className="flex items-center gap-2">
                              <span className="text-xs font-bold font-mono" style={{ color: catColor || 'var(--accent)' }}>
                                {ev.time.slice(0, 5)}{ev.end_time ? ` - ${ev.end_time.slice(0, 5)}` : ''}
                              </span>
                              <h4 className="text-sm font-semibold truncate" style={{ color: 'var(--text-primary)' }}>{ev.title}</h4>
                              {ev.notify_telegram === false && <span title="Sin aviso Telegram"><BellOff size={10} style={{ color: 'var(--text-secondary)', opacity: 0.5 }} /></span>}
                            </div>
                            {ev.description && <p className="text-[10px] mt-1" style={{ color: 'var(--text-primary)', opacity: 0.8 }}>{ev.description}</p>}
                            {ev.location && (
                              <p className="text-[10px] mt-0.5 flex items-center gap-1" style={{ color: 'var(--text-secondary)' }}>
                                <MapPin size={9} /> {ev.location}
                              </p>
                            )}
                            <p className="text-[10px] mt-1" style={{ color: 'var(--text-secondary)' }}>
                              {ev.created_by}
                              {cat && (
                                <span className="ml-1.5 px-1.5 py-0.5 rounded text-[9px] font-medium" style={{ backgroundColor: cat.color + '20', color: cat.color }}>
                                  {cat.name}
                                </span>
                              )}
                              {ev.recurrence && ev.recurrence !== 'none' && (
                                <span className="ml-1.5 px-1 py-0.5 rounded text-[9px]" style={{ backgroundColor: 'var(--accent-alpha)', color: 'var(--accent)' }}>
                                  {recurrenceLabel(ev.recurrence)}
                                </span>
                              )}
                            </p>
                            {ev.invitees.length > 0 && (
                              <p className="text-[10px] mt-1" style={{ color: 'var(--text-secondary)' }}>
                                {ev.invitees.map(inv => {
                                  const accepted = ev.accepted.includes(inv)
                                  const declined = ev.declined.includes(inv)
                                  return <span key={inv} className="mr-1.5" style={{ color: accepted ? 'var(--success)' : declined ? 'var(--danger)' : 'var(--text-secondary)' }}>{inv}</span>
                                })}
                              </p>
                            )}
                          </div>
                          <div className="flex items-center gap-1 shrink-0">
                            {username && !ev.accepted.includes(username) && !ev.declined.includes(username) && ev.created_by !== username && (
                              <>
                                <button onClick={async () => { await acceptEvent(ev.id, username); await loadData() }}
                                  className="p-1 rounded-lg" style={{ color: 'var(--success)' }} title="Aceptar"><CheckCircle2 size={14} /></button>
                                <button onClick={async () => { await declineEvent(ev.id, username); await loadData() }}
                                  className="p-1 rounded-lg" style={{ color: 'var(--danger)' }} title="Rechazar"><XCircle size={14} /></button>
                              </>
                            )}
                            <button onClick={() => onEdit(ev)} className="p-1 rounded-lg hover:opacity-80" style={{ color: 'var(--text-secondary)' }}><Pencil size={12} /></button>
                            <button onClick={async () => { await deleteEvent(ev.id); await loadData() }}
                              className="p-1 rounded-lg hover:opacity-80" style={{ color: 'var(--danger)' }}><Trash2 size={12} /></button>
                          </div>
                        </div>
                      </div>
                    )
                  })}

                  {selectedTasks.map(t => (
                    <div key={t.id} className="rounded-xl p-4" style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--warning)' + '40' }}>
                      <div className="flex items-center gap-2">
                        <ClipboardList size={12} style={{ color: 'var(--warning)' }} />
                        <span className="text-xs font-medium" style={{ color: 'var(--text-primary)' }}>{t.title}</span>
                        {t.due_time && <span className="text-[10px] font-mono" style={{ color: 'var(--warning)' }}>{t.due_time.slice(0, 5)}</span>}
                        <span className="text-[10px] ml-auto px-1.5 py-0.5 rounded" style={{ backgroundColor: 'var(--warning)' + '20', color: 'var(--warning)' }}>Tarea</span>
                      </div>
                    </div>
                  ))}
                </>
              ) : (
                <div className="rounded-xl p-6 text-center" style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}>
                  <Calendar size={24} className="mx-auto mb-2" style={{ color: 'var(--text-secondary)', opacity: 0.4 }} />
                  <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>Selecciona un dia para ver sus eventos</p>
                </div>
              )}
            </div>
  )
}

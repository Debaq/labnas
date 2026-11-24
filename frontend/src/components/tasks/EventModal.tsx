import { Plus, Trash2, X } from 'lucide-react'
import { createCategory, deleteCategory, updateCategory } from '../../api'
import type { EventCategory } from '../../types'
import type { EventForm } from './useEventForm'

interface Props {
  form: EventForm
  categories: EventCategory[]
  allUsers: string[]
  username?: string
  loadData: () => Promise<void>
}

export default function EventModal({ form, categories, allUsers, username, loadData }: Props) {
  const {
    showEventModal, setShowEventModal, eventTitle, setEventTitle, eventDate, setEventDate, eventTime, setEventTime,
    eventInvitees, setEventInvitees, inviteeSearch, setInviteeSearch, eventEndTime, setEventEndTime,
    eventDescription, setEventDescription, eventLocation, setEventLocation, eventRemind, setEventRemind,
    eventNotifyTg, setEventNotifyTg, eventRecurrence, setEventRecurrence, eventRecurrenceEnd, setEventRecurrenceEnd,
    editingEventId, eventCategory, setEventCategory, showCategoryManager, setShowCategoryManager,
    newCatName, setNewCatName, newCatColor, setNewCatColor,
  } = form
  if (!showEventModal) return null
  return (
            <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
              <div className="rounded-xl p-6 w-full max-w-md mx-4" style={{ backgroundColor: 'var(--bg-secondary)', border: '1px solid var(--border)' }}>
                <div className="flex items-center justify-between mb-4">
                  <h3 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>{editingEventId ? 'Editar Evento' : 'Nuevo Evento'}</h3>
                  <button onClick={() => setShowEventModal(false)} style={{ color: 'var(--text-secondary)' }}><X size={20} /></button>
                </div>
                <div className="space-y-3">
                  <div>
                    <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Titulo</label>
                    <input value={eventTitle} onChange={e => setEventTitle(e.target.value)} placeholder="Reunion de equipo"
                      className="w-full px-3 py-2 rounded-lg text-sm outline-none"
                      style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }} />
                  </div>
                  <div>
                    <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Descripcion</label>
                    <textarea value={eventDescription} onChange={e => setEventDescription(e.target.value)} placeholder="Detalles del evento (opcional)"
                      rows={2} className="w-full px-3 py-2 rounded-lg text-sm outline-none resize-none"
                      style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }} />
                  </div>
                  <div className="grid grid-cols-3 gap-3">
                    <div>
                      <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Fecha</label>
                      <input type="date" value={eventDate} onChange={e => setEventDate(e.target.value)}
                        className="w-full px-3 py-2 rounded-lg text-sm outline-none"
                        style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }} />
                    </div>
                    <div>
                      <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Inicio</label>
                      <input type="time" value={eventTime} onChange={e => setEventTime(e.target.value)}
                        className="w-full px-3 py-2 rounded-lg text-sm outline-none"
                        style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }} />
                    </div>
                    <div>
                      <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Fin</label>
                      <input type="time" value={eventEndTime} onChange={e => setEventEndTime(e.target.value)}
                        className="w-full px-3 py-2 rounded-lg text-sm outline-none"
                        style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }} />
                    </div>
                  </div>
                  <div>
                    <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Lugar</label>
                    <input value={eventLocation} onChange={e => setEventLocation(e.target.value)} placeholder="Sala de reuniones, Lab 3, etc."
                      className="w-full px-3 py-2 rounded-lg text-sm outline-none"
                      style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }} />
                  </div>
                  <div>
                    <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Invitados</label>
                    {/* Selected chips */}
                    {eventInvitees.length > 0 && (
                      <div className="flex flex-wrap gap-1.5 mb-2">
                        {eventInvitees.map(name => (
                          <span key={name} className="flex items-center gap-1 text-xs px-2 py-0.5 rounded-full"
                            style={{ backgroundColor: name === 'all' ? 'var(--accent-alpha)' : 'var(--bg-tertiary)', color: name === 'all' ? 'var(--accent)' : 'var(--text-primary)' }}>
                            {name === 'all' ? 'Todos' : name}
                            <button type="button" onClick={() => setEventInvitees(prev => prev.filter(n => n !== name))} className="hover:opacity-60"><X size={10} /></button>
                          </span>
                        ))}
                      </div>
                    )}
                    {/* Search input */}
                    {!eventInvitees.includes('all') && (
                      <div className="relative">
                        <input
                          value={inviteeSearch}
                          onChange={e => setInviteeSearch(e.target.value)}
                          placeholder={eventInvitees.length > 0 ? 'Agregar otro...' : 'Buscar usuario...'}
                          className="w-full px-3 py-2 rounded-lg text-sm outline-none"
                          style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
                        />
                        {/* Suggestions */}
                        {inviteeSearch.length > 0 && (() => {
                          const q = inviteeSearch.toLowerCase()
                          const suggestions = allUsers.filter(u => u.toLowerCase().includes(q) && !eventInvitees.includes(u) && u !== username)
                          const showAll = 'todos'.includes(q) || 'all'.includes(q)
                          if (!suggestions.length && !showAll) return null
                          return (
                            <div className="absolute z-10 left-0 right-0 rounded-lg mt-1 overflow-hidden max-h-36 overflow-y-auto shadow-lg"
                              style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--border)' }}>
                              {showAll && (
                                <button type="button" onClick={() => { setEventInvitees(['all']); setInviteeSearch('') }}
                                  className="w-full text-left px-3 py-2 text-xs font-medium hover:opacity-80"
                                  style={{ color: 'var(--accent)', backgroundColor: 'var(--accent-alpha)' }}>
                                  Todos los usuarios
                                </button>
                              )}
                              {suggestions.map(u => (
                                <button type="button" key={u} onClick={() => { setEventInvitees(prev => [...prev, u]); setInviteeSearch('') }}
                                  className="w-full text-left px-3 py-2 text-xs hover:opacity-80 transition-all"
                                  style={{ color: 'var(--text-primary)', borderTop: '1px solid var(--border)' }}>
                                  {u}
                                </button>
                              ))}
                            </div>
                          )
                        })()}
                      </div>
                    )}
                  </div>
                  {/* Categoria */}
                  <div>
                    <div className="flex items-center justify-between mb-1">
                      <label className="block text-xs font-medium" style={{ color: 'var(--text-secondary)' }}>Categoria</label>
                      <button type="button" onClick={() => setShowCategoryManager(!showCategoryManager)}
                        className="text-[10px]" style={{ color: 'var(--accent)' }}>
                        {showCategoryManager ? 'Cerrar' : 'Gestionar'}
                      </button>
                    </div>
                    <div className="flex flex-wrap gap-1.5">
                      <button type="button" onClick={() => setEventCategory('')}
                        className="px-2.5 py-1 rounded-full text-xs font-medium transition-all"
                        style={{
                          backgroundColor: !eventCategory ? 'var(--text-secondary)' : 'var(--bg-tertiary)',
                          color: !eventCategory ? '#fff' : 'var(--text-secondary)',
                        }}>
                        Sin categoria
                      </button>
                      {categories.map(cat => (
                        <button type="button" key={cat.id} onClick={() => setEventCategory(cat.id)}
                          className="px-2.5 py-1 rounded-full text-xs font-medium transition-all"
                          style={{
                            backgroundColor: eventCategory === cat.id ? cat.color : cat.color + '20',
                            color: eventCategory === cat.id ? '#fff' : cat.color,
                            border: eventCategory === cat.id ? `2px solid ${cat.color}` : '2px solid transparent',
                          }}>
                          {cat.name}
                        </button>
                      ))}
                    </div>
                    {/* Category manager inline */}
                    {showCategoryManager && (
                      <div className="mt-2 p-3 rounded-lg space-y-2" style={{ backgroundColor: 'var(--bg-tertiary)', border: '1px solid var(--border)' }}>
                        <div className="flex gap-2">
                          <input value={newCatName} onChange={e => setNewCatName(e.target.value)} placeholder="Nueva categoria"
                            className="flex-1 px-2 py-1 rounded-lg text-xs outline-none"
                            style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }} />
                          <input type="color" value={newCatColor} onChange={e => setNewCatColor(e.target.value)}
                            className="w-8 h-7 rounded cursor-pointer" style={{ border: '1px solid var(--border)' }} />
                          <button type="button" onClick={async () => {
                            if (!newCatName.trim()) return
                            await createCategory(newCatName, newCatColor)
                            setNewCatName(''); await loadData()
                          }} className="px-2 py-1 rounded-lg text-xs font-medium"
                            style={{ backgroundColor: 'var(--accent)', color: '#fff' }}>
                            <Plus size={12} />
                          </button>
                        </div>
                        {categories.map(cat => (
                          <div key={cat.id} className="flex items-center gap-2 text-xs py-1">
                            <input type="color" defaultValue={cat.color}
                              onBlur={async (e) => { if (e.target.value !== cat.color) { await updateCategory(cat.id, cat.name, e.target.value); await loadData() } }}
                              className="w-5 h-5 rounded cursor-pointer border-0" />
                            <input type="text" defaultValue={cat.name}
                              onBlur={async (e) => { if (e.target.value.trim() && e.target.value !== cat.name) { await updateCategory(cat.id, e.target.value, cat.color); await loadData() } }}
                              className="flex-1 px-1 py-0.5 rounded text-xs outline-none"
                              style={{ backgroundColor: 'transparent', color: 'var(--text-primary)' }} />
                            <button type="button" onClick={async () => { await deleteCategory(cat.id); await loadData() }}
                              style={{ color: 'var(--danger)' }}><Trash2 size={12} /></button>
                          </div>
                        ))}
                      </div>
                    )}
                  </div>

                  <div className="grid grid-cols-2 gap-3">
                    <div>
                      <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Avisar (min antes)</label>
                      <input type="number" min={1} value={eventRemind} onChange={e => setEventRemind(parseInt(e.target.value) || 15)}
                        className="w-full px-3 py-2 rounded-lg text-sm outline-none"
                        style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }} />
                    </div>
                    <div>
                      <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Repetir</label>
                      <select value={eventRecurrence.startsWith('days:') ? 'custom' : eventRecurrence} onChange={e => {
                          const v = e.target.value
                          if (v === 'custom') {
                            // Default: día actual
                            const d = eventDate ? new Date(eventDate + 'T12:00').getDay() : new Date().getDay()
                            const chrono = d === 0 ? 6 : d - 1 // JS Sunday=0 → chrono Mon=0
                            setEventRecurrence(`days:${chrono}`)
                          } else {
                            setEventRecurrence(v)
                          }
                        }}
                        className="w-full px-3 py-2 rounded-lg text-sm outline-none cursor-pointer"
                        style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}>
                        <option value="">No repetir</option>
                        <option value="daily">Diario</option>
                        <option value="weekdays">Dias habiles (Lun-Vie)</option>
                        <option value="weekly">Semanal</option>
                        <option value="monthly">Mensual</option>
                        <option value="custom">Dias especificos...</option>
                      </select>
                      {eventRecurrence.startsWith('days:') && (
                        <div className="flex gap-1 mt-2">
                          {['Lun','Mar','Mie','Jue','Vie','Sab','Dom'].map((day, i) => {
                            const selected = eventRecurrence.replace('days:', '').split(',').includes(String(i))
                            return (
                              <button key={i} type="button" onClick={() => {
                                const current = eventRecurrence.replace('days:', '').split(',').filter(Boolean).map(Number)
                                const next = selected ? current.filter(d => d !== i) : [...current, i].sort()
                                if (next.length === 0) return
                                setEventRecurrence(`days:${next.join(',')}`)
                              }}
                                className="flex-1 py-1.5 rounded-lg text-[11px] font-medium transition-all"
                                style={{
                                  backgroundColor: selected ? 'var(--accent)' : 'var(--input-bg)',
                                  color: selected ? '#fff' : 'var(--text-secondary)',
                                  border: `1px solid ${selected ? 'var(--accent)' : 'var(--input-border)'}`,
                                }}>
                                {day}
                              </button>
                            )
                          })}
                        </div>
                      )}
                    </div>
                  </div>
                  {eventRecurrence && (
                    <div>
                      <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Repetir hasta (opcional)</label>
                      <input type="date" value={eventRecurrenceEnd} onChange={e => setEventRecurrenceEnd(e.target.value)}
                        className="w-full px-3 py-2 rounded-lg text-sm outline-none"
                        style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }} />
                    </div>
                  )}
                  <label className="flex items-center gap-2 cursor-pointer">
                    <input type="checkbox" checked={eventNotifyTg} onChange={e => setEventNotifyTg(e.target.checked)}
                      style={{ accentColor: 'var(--accent)' }} />
                    <span className="text-xs" style={{ color: 'var(--text-secondary)' }}>Avisar por Telegram</span>
                  </label>
                </div>
                <div className="flex justify-end gap-3 mt-5">
                  <button onClick={() => setShowEventModal(false)} className="px-4 py-2 rounded-lg text-sm font-medium"
                    style={{ color: 'var(--text-secondary)', border: '1px solid var(--border)' }}>Cancelar</button>
                  <button
                    onClick={() => form.save(loadData)}
                    disabled={!eventTitle.trim() || !eventDate || !eventTime}
                    className="px-4 py-2 rounded-lg text-sm font-medium"
                    style={{ backgroundColor: 'var(--accent)', color: '#ffffff' }}
                  >{editingEventId ? 'Guardar' : 'Crear'}</button>
                </div>
              </div>
            </div>
  )
}

import { Plus, ChevronLeft, ChevronRight } from 'lucide-react'
import type { Task, CalendarEvent, EventCategory } from '../../types'
import { MONTH_NAMES, DAY_NAMES, buildCalendarCells } from './utils'
import type { EventForm } from './useEventForm'
import CalendarDayPanel from './CalendarDayPanel'
import EventModal from './EventModal'

interface Props {
  events: CalendarEvent[]
  tasks: Task[]
  categories: EventCategory[]
  allUsers: string[]
  username?: string
  calendarMonth: Date
  setCalendarMonth: (d: Date) => void
  selectedDay: string | null
  setSelectedDay: (d: string | null) => void
  eventForm: EventForm
  loadData: () => Promise<void>
}

export default function CalendarView({ events, tasks, categories, allUsers, username, calendarMonth, setCalendarMonth, selectedDay, setSelectedDay, eventForm, loadData }: Props) {
  const today = new Date()
  const todayStr = today.toISOString().slice(0, 10)
  const year = calendarMonth.getFullYear()
  const month = calendarMonth.getMonth()
  const cells = buildCalendarCells(calendarMonth)

  // Map events by date
  const eventsByDate: Record<string, CalendarEvent[]> = {}
  for (const ev of events) {
    if (!eventsByDate[ev.date]) eventsByDate[ev.date] = []
    eventsByDate[ev.date].push(ev)
  }
  // Also map tasks with due_date
  const tasksByDate: Record<string, Task[]> = {}
  for (const t of tasks) {
    if (t.due_date) {
      if (!tasksByDate[t.due_date]) tasksByDate[t.due_date] = []
      tasksByDate[t.due_date].push(t)
    }
  }

  // Category color helper
  const catMap = new Map(categories.map(c => [c.id, c]))
  const getCatColor = (ev: CalendarEvent) => ev.category ? catMap.get(ev.category)?.color : undefined

  const monthNames = MONTH_NAMES
  const dayNames = DAY_NAMES

  const selectedEvents = selectedDay ? (eventsByDate[selectedDay] || []).sort((a, b) => a.time.localeCompare(b.time)) : []
  const selectedTasks = selectedDay ? (tasksByDate[selectedDay] || []) : []

  return (
  <div className="space-y-4">
    {/* Header */}
    <div className="flex items-center justify-between">
      <div className="flex items-center gap-3">
        <button onClick={() => setCalendarMonth(new Date(year, month - 1, 1))} className="p-1.5 rounded-lg hover:opacity-80" style={{ color: 'var(--text-secondary)' }}>
          <ChevronLeft size={18} />
        </button>
        <h2 className="text-base font-semibold min-w-[180px] text-center" style={{ color: 'var(--text-primary)' }}>
          {monthNames[month]} {year}
        </h2>
        <button onClick={() => setCalendarMonth(new Date(year, month + 1, 1))} className="p-1.5 rounded-lg hover:opacity-80" style={{ color: 'var(--text-secondary)' }}>
          <ChevronRight size={18} />
        </button>
        <button
          onClick={() => { setCalendarMonth(new Date(today.getFullYear(), today.getMonth(), 1)); setSelectedDay(todayStr) }}
          className="text-xs px-2.5 py-1 rounded-lg font-medium"
          style={{ color: 'var(--accent)', border: '1px solid var(--border)' }}
        >Hoy</button>
      </div>
      <button
        onClick={() => {
          eventForm.openNew(selectedDay || todayStr)
        }}
        className="flex items-center gap-2 px-4 py-2 rounded-lg text-sm font-medium"
        style={{ backgroundColor: 'var(--accent)', color: '#ffffff' }}
      >
        <Plus size={16} />Nuevo Evento
      </button>
    </div>

    <div className="grid grid-cols-1 lg:grid-cols-[1fr_320px] gap-4">
      {/* Calendar Grid */}
      <div className="rounded-xl overflow-hidden" style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}>
        {/* Day headers */}
        <div className="grid grid-cols-7">
          {dayNames.map(d => (
            <div key={d} className="py-2 text-center text-[10px] font-semibold uppercase tracking-wider" style={{ color: 'var(--text-secondary)', borderBottom: '1px solid var(--border)' }}>
              {d}
            </div>
          ))}
        </div>
        {/* Day cells */}
        <div className="grid grid-cols-7">
          {cells.map((cell, i) => {
            const isToday = cell.dateStr === todayStr
            const isSelected = cell.dateStr === selectedDay
            const dayEvents = eventsByDate[cell.dateStr] || []
            const dayTasks = tasksByDate[cell.dateStr] || []
            const hasItems = dayEvents.length > 0 || dayTasks.length > 0
            return (
              <div
                key={i}
                onClick={() => setSelectedDay(cell.dateStr === selectedDay ? null : cell.dateStr)}
                className="relative p-1.5 min-h-[80px] cursor-pointer transition-all duration-150 hover:opacity-80"
                style={{
                  borderRight: (i + 1) % 7 !== 0 ? '1px solid var(--border)' : undefined,
                  borderBottom: i < cells.length - 7 ? '1px solid var(--border)' : undefined,
                  backgroundColor: isSelected ? 'var(--accent-alpha)' : isToday ? 'var(--success-alpha)' : 'transparent',
                  opacity: cell.isCurrentMonth ? 1 : 0.35,
                }}
              >
                <div className="flex items-center justify-between mb-1">
                  <span
                    className={'text-xs font-medium' + (isToday ? ' w-5 h-5 rounded-full flex items-center justify-center' : '')}
                    style={{
                      color: isToday ? '#fff' : isSelected ? 'var(--accent)' : 'var(--text-primary)',
                      backgroundColor: isToday ? 'var(--accent)' : undefined,
                    }}
                  >
                    {cell.day}
                  </span>
                  {hasItems && !isSelected && (
                    <span className="text-[9px] font-mono" style={{ color: 'var(--accent)' }}>{dayEvents.length + dayTasks.length}</span>
                  )}
                </div>
                {/* Event dots */}
                <div className="space-y-0.5">
                  {dayEvents.slice(0, 3).map(ev => {
                    const catColor = getCatColor(ev)
                    return (
                      <div key={ev.id} className="text-[9px] leading-tight truncate py-0.5 rounded"
                        style={{
                          backgroundColor: catColor ? catColor + '20' : 'var(--accent-alpha)',
                          color: catColor || 'var(--accent)',
                          borderLeft: `3px solid ${catColor || 'var(--accent)'}`,
                          paddingLeft: '4px',
                          paddingRight: '4px',
                        }}>
                        {ev.time.slice(0, 5)} {ev.title}
                      </div>
                    )
                  })}
                  {dayTasks.slice(0, Math.max(0, 3 - dayEvents.length)).map(t => (
                    <div key={t.id} className="text-[9px] leading-tight truncate px-1 py-0.5 rounded" style={{ backgroundColor: 'var(--warning)' + '20', color: 'var(--warning)' }}>
                      {t.due_time ? t.due_time.slice(0, 5) + ' ' : ''}{t.title}
                    </div>
                  ))}
                  {(dayEvents.length + dayTasks.length) > 3 && (
                    <span className="text-[9px] px-1" style={{ color: 'var(--text-secondary)' }}>+{dayEvents.length + dayTasks.length - 3} mas</span>
                  )}
                </div>
              </div>
            )
          })}
        </div>
      </div>

      <CalendarDayPanel
        selectedDay={selectedDay}
        selectedEvents={selectedEvents}
        selectedTasks={selectedTasks}
        catMap={catMap}
        getCatColor={getCatColor}
        username={username}
        loadData={loadData}
        onEdit={eventForm.openEdit}
      />
    </div>

    <EventModal form={eventForm} categories={categories} allUsers={allUsers} username={username} loadData={loadData} />
  </div>
  )
}

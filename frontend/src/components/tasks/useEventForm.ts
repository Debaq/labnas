import { useState } from 'react'
import { createEvent, updateEvent } from '../../api'
import type { CalendarEvent } from '../../types'

export function useEventForm() {
  const [showEventModal, setShowEventModal] = useState(false)
  const [eventTitle, setEventTitle] = useState('')
  const [eventDate, setEventDate] = useState('')
  const [eventTime, setEventTime] = useState('')
  const [eventInvitees, setEventInvitees] = useState<string[]>([])
  const [inviteeSearch, setInviteeSearch] = useState('')
  const [eventEndTime, setEventEndTime] = useState('')
  const [eventDescription, setEventDescription] = useState('')
  const [eventLocation, setEventLocation] = useState('')
  const [eventRemind, setEventRemind] = useState(15)
  const [eventNotifyTg, setEventNotifyTg] = useState(true)
  const [eventRecurrence, setEventRecurrence] = useState('')
  const [eventRecurrenceEnd, setEventRecurrenceEnd] = useState('')
  const [editingEventId, setEditingEventId] = useState<string | null>(null)
  const [eventCategory, setEventCategory] = useState('')
  const [showCategoryManager, setShowCategoryManager] = useState(false)
  const [newCatName, setNewCatName] = useState('')
  const [newCatColor, setNewCatColor] = useState('#7aa2f7')

  function openNew(date: string) {
    setShowEventModal(true); setEventTitle(''); setEventTime('')
    setEventDate(date)
    setEventInvitees([]); setInviteeSearch(''); setEventEndTime(''); setEventDescription(''); setEventLocation('')
    setEventRemind(15); setEventNotifyTg(true); setEventRecurrence(''); setEventRecurrenceEnd(''); setEventCategory(''); setEditingEventId(null)
  }

  function openEdit(ev: CalendarEvent) {
    setEditingEventId(ev.id); setEventTitle(ev.title); setEventDate(ev.date); setEventTime(ev.time)
    setEventEndTime(ev.end_time || ''); setEventDescription(ev.description || ''); setEventLocation(ev.location || '')
    setEventInvitees(ev.invitees); setEventRemind(ev.remind_before_min); setEventNotifyTg(ev.notify_telegram ?? true)
    setEventRecurrence(ev.recurrence || ''); setEventRecurrenceEnd(ev.recurrence_end || ''); setEventCategory(ev.category || '')
    setInviteeSearch(''); setShowEventModal(true)
  }

  async function save(loadData: () => Promise<void>) {
    if (!eventTitle.trim() || !eventDate || !eventTime) return
    const data = {
      title: eventTitle, date: eventDate, time: eventTime, end_time: eventEndTime || undefined,
      description: eventDescription, location: eventLocation || undefined, invitees: eventInvitees,
      remind_before_min: eventRemind, notify_telegram: eventNotifyTg,
      recurrence: eventRecurrence || undefined, recurrence_end: eventRecurrenceEnd || null,
      category: eventCategory || undefined,
    }
    if (editingEventId) {
      await updateEvent(editingEventId, data)
    } else {
      await createEvent(data)
    }
    setShowEventModal(false)
    await loadData()
  }

  return {
    showEventModal, setShowEventModal, eventTitle, setEventTitle, eventDate, setEventDate, eventTime, setEventTime,
    eventInvitees, setEventInvitees, inviteeSearch, setInviteeSearch, eventEndTime, setEventEndTime,
    eventDescription, setEventDescription, eventLocation, setEventLocation, eventRemind, setEventRemind,
    eventNotifyTg, setEventNotifyTg, eventRecurrence, setEventRecurrence, eventRecurrenceEnd, setEventRecurrenceEnd,
    editingEventId, eventCategory, setEventCategory, showCategoryManager, setShowCategoryManager,
    newCatName, setNewCatName, newCatColor, setNewCatColor, openNew, openEdit, save,
  }
}

export type EventForm = ReturnType<typeof useEventForm>

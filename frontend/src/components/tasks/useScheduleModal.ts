import { useState } from 'react'
import { scheduleTask } from '../../api'
import type { Task } from '../../types'

export function useScheduleModal() {
  const [showScheduleModal, setShowScheduleModal] = useState(false)
  const [scheduleTaskId, setScheduleTaskId] = useState('')
  const [scheduleDate, setScheduleDate] = useState('')
  const [scheduleTime, setScheduleTime] = useState('')
  const [scheduleMissing, setScheduleMissing] = useState<'date' | 'time' | 'both'>('both')

  function open(id: string, task: Task | undefined) {
    const hasDate = !!task?.due_date
    const hasTime = !!task?.due_time
    setScheduleTaskId(id)
    setScheduleDate(task?.due_date || '')
    setScheduleTime(task?.due_time || '')
    setScheduleMissing(!hasDate && !hasTime ? 'both' : !hasDate ? 'date' : 'time')
    setShowScheduleModal(true)
  }

  async function confirm(loadData: () => Promise<void>) {
    if (!scheduleDate || !scheduleTime) return
    try {
      await scheduleTask(scheduleTaskId, { date: scheduleDate, time: scheduleTime })
      setShowScheduleModal(false)
      loadData()
    } catch { /* silenciar */ }
  }

  return { showScheduleModal, setShowScheduleModal, scheduleDate, setScheduleDate, scheduleTime, setScheduleTime, scheduleMissing, open, confirm }
}

export type ScheduleModalState = ReturnType<typeof useScheduleModal>

import { useEffect, useState } from 'react'
import { X, CheckCircle2, AlertCircle, Info } from 'lucide-react'

export type ToastType = 'success' | 'error' | 'info'

export interface ToastData {
  id: string
  message: string
  type: ToastType
}

interface ToastProps {
  toast: ToastData
  onClose: (id: string) => void
}

export default function Toast({ toast, onClose }: ToastProps) {
  const [exiting, setExiting] = useState(false)

  useEffect(() => {
    const timer = setTimeout(() => {
      setExiting(true)
      setTimeout(() => onClose(toast.id), 300)
    }, 3000)
    return () => clearTimeout(timer)
  }, [toast.id, onClose])

  const colors: Record<ToastType, { bg: string; border: string; icon: string }> = {
    success: { bg: 'var(--success)', border: 'var(--success)', icon: 'var(--success)' },
    error: { bg: 'var(--danger)', border: 'var(--danger)', icon: 'var(--danger)' },
    info: { bg: 'var(--accent)', border: 'var(--accent)', icon: 'var(--accent)' },
  }

  const Icon = toast.type === 'success' ? CheckCircle2 : toast.type === 'error' ? AlertCircle : Info
  const c = colors[toast.type]

  return (
    <div
      className="flex items-center gap-3 px-4 py-3 rounded-xl shadow-lg max-w-sm transition-all duration-300"
      style={{
        backgroundColor: 'var(--card-bg)',
        border: `1px solid ${c.border}`,
        transform: exiting ? 'translateX(120%)' : 'translateX(0)',
        opacity: exiting ? 0 : 1,
        animation: exiting ? undefined : 'toast-slide-in 0.3s ease-out',
      }}
    >
      <Icon size={18} style={{ color: c.icon, flexShrink: 0 }} />
      <span className="text-sm flex-1" style={{ color: 'var(--text-primary)' }}>
        {toast.message}
      </span>
      <button
        onClick={() => {
          setExiting(true)
          setTimeout(() => onClose(toast.id), 300)
        }}
        className="p-0.5 rounded hover:opacity-70 transition-opacity"
        style={{ color: 'var(--text-secondary)' }}
      >
        <X size={14} />
      </button>
    </div>
  )
}

import { createContext, useContext, useState, useCallback, type ReactNode } from 'react'
import Toast, { type ToastType, type ToastData } from './Toast'

interface ToastContextValue {
  addToast: (message: string, type?: ToastType) => void
}

const ToastContext = createContext<ToastContextValue | undefined>(undefined)

let toastCounter = 0

export function ToastProvider({ children }: { children: ReactNode }) {
  const [toasts, setToasts] = useState<ToastData[]>([])

  const addToast = useCallback((message: string, type: ToastType = 'info') => {
    const id = `toast-${++toastCounter}`
    setToasts(prev => {
      // Maximo 3 toasts visibles, eliminar el mas antiguo si se excede
      const updated = [...prev, { id, message, type }]
      if (updated.length > 3) {
        return updated.slice(-3)
      }
      return updated
    })
  }, [])

  const removeToast = useCallback((id: string) => {
    setToasts(prev => prev.filter(t => t.id !== id))
  }, [])

  return (
    <ToastContext.Provider value={{ addToast }}>
      {children}
      {/* Toast container - esquina inferior derecha */}
      {toasts.length > 0 && (
        <div
          className="fixed bottom-6 right-6 z-50 flex flex-col gap-3"
          style={{ pointerEvents: 'none' }}
        >
          {toasts.map(toast => (
            <div key={toast.id} style={{ pointerEvents: 'auto' }}>
              <Toast toast={toast} onClose={removeToast} />
            </div>
          ))}
        </div>
      )}
      {/* Estilos de animacion para los toasts */}
      <style>{`
        @keyframes toast-slide-in {
          from {
            transform: translateX(120%);
            opacity: 0;
          }
          to {
            transform: translateX(0);
            opacity: 1;
          }
        }
      `}</style>
    </ToastContext.Provider>
  )
}

export function useToast(): ToastContextValue {
  const context = useContext(ToastContext)
  if (!context) {
    throw new Error('useToast must be used within a ToastProvider')
  }
  return context
}

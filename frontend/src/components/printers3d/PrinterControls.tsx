import { useState } from 'react'
import { ArrowLeft, Send, Loader2, Flame, Move, ArrowUp, ArrowDown, ArrowRight, Home, FileText, RefreshCw, Play, Trash2 } from 'lucide-react'
import {
  preheat3D,
  homeAxes3D,
  jog3D,
  sendGcode3D,
  printFile3D,
  deletePrinterFile,
} from '../../api'
import type { PrinterFileInfo } from '../../types'
import { formatFileSize } from './utils'

interface Props {
  printerId: string
  files: PrinterFileInfo[]
  loadingFiles: boolean
  onLoadFiles: () => void
  showFeedback: (printerId: string, message: string) => void
}

export default function PrinterControls({ printerId, files, loadingFiles, onLoadFiles, showFeedback }: Props) {
  const [jogDistance, setJogDistance] = useState(10)
  const [gcodeInput, setGcodeInput] = useState('')
  const [preheatHotend, setPreheatHotend] = useState(200)
  const [preheatBed, setPreheatBed] = useState(60)

  async function handlePreheat() {
    try {
      await preheat3D(printerId, preheatHotend, preheatBed)
      showFeedback(printerId, `Precalentando: ${preheatHotend}°C / ${preheatBed}°C`)
    } catch (err) {
      console.error('Error preheat:', err)
      showFeedback(printerId, 'Error al precalentar')
    }
  }

  async function handleHome() {
    try {
      await homeAxes3D(printerId)
      showFeedback(printerId, 'Home enviado')
    } catch (err) {
      console.error('Error home:', err)
      showFeedback(printerId, 'Error al hacer home')
    }
  }

  async function handleJog(x: number, y: number, z: number) {
    try {
      await jog3D(printerId, x, y, z)
    } catch (err) {
      console.error('Error jog:', err)
      showFeedback(printerId, 'Error al mover')
    }
  }

  async function handleSendGcode() {
    const cmd = gcodeInput.trim()
    if (!cmd) return
    try {
      await sendGcode3D(printerId, cmd)
      showFeedback(printerId, `Enviado: ${cmd}`)
      setGcodeInput('')
    } catch (err) {
      console.error('Error gcode:', err)
      showFeedback(printerId, 'Error al enviar G-code')
    }
  }

  async function handlePrintFile(filename: string) {
    try {
      await printFile3D(printerId, filename)
      showFeedback(printerId, `Imprimiendo '${filename}'`)
    } catch (err) {
      console.error('Error print file:', err)
      showFeedback(printerId, 'Error al imprimir archivo')
    }
  }

  async function handleDeleteFile(filename: string) {
    if (!confirm(`Eliminar '${filename}'?`)) return
    try {
      await deletePrinterFile(printerId, filename)
      onLoadFiles()
    } catch (err) {
      console.error('Error delete file:', err)
    }
  }

  return (
      <div className="px-6 pb-6 space-y-4" style={{ borderTop: '1px solid var(--border)' }}>
        {/* Precalentar */}
        <div className="pt-4">
          <h4 className="text-xs font-semibold mb-2 flex items-center gap-1.5" style={{ color: 'var(--text-secondary)' }}>
            <Flame size={14} /> Precalentar
          </h4>
          <div className="flex items-center gap-2 flex-wrap">
            <div className="flex items-center gap-1">
              <span className="text-xs" style={{ color: 'var(--text-secondary)' }}>Hotend:</span>
              <input
                type="number"
                value={preheatHotend}
                onChange={(e) => setPreheatHotend(parseInt(e.target.value) || 0)}
                className="w-16 px-2 py-1 rounded text-xs text-center outline-none"
                style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
              />
              <span className="text-xs" style={{ color: 'var(--text-secondary)' }}>°C</span>
            </div>
            <div className="flex items-center gap-1">
              <span className="text-xs" style={{ color: 'var(--text-secondary)' }}>Cama:</span>
              <input
                type="number"
                value={preheatBed}
                onChange={(e) => setPreheatBed(parseInt(e.target.value) || 0)}
                className="w-16 px-2 py-1 rounded text-xs text-center outline-none"
                style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
              />
              <span className="text-xs" style={{ color: 'var(--text-secondary)' }}>°C</span>
            </div>
            <button
              onClick={() => handlePreheat()}
              className="flex items-center gap-1 px-3 py-1.5 rounded-lg text-xs font-medium hover:opacity-90"
              style={{ backgroundColor: 'var(--danger)', color: '#ffffff' }}
            >
              <Flame size={12} /> Calentar
            </button>
            <button
              onClick={async () => {
                try {
                  await preheat3D(printerId, 0, 0)
                  showFeedback(printerId, 'Enfriando...')
                } catch {}
              }}
              className="px-3 py-1.5 rounded-lg text-xs font-medium hover:opacity-90"
              style={{ backgroundColor: 'var(--bg-tertiary)', color: 'var(--text-primary)', border: '1px solid var(--border)' }}
            >
              Enfriar
            </button>
          </div>
        </div>

        {/* Jog Controls */}
        <div>
          <h4 className="text-xs font-semibold mb-2 flex items-center gap-1.5" style={{ color: 'var(--text-secondary)' }}>
            <Move size={14} /> Control de ejes
          </h4>
          <div className="flex items-start gap-4 flex-wrap">
            {/* XY pad */}
            <div className="grid grid-cols-3 gap-1" style={{ width: 120 }}>
              <div />
              <button
                onClick={() => handleJog(0, jogDistance, 0)}
                className="p-2 rounded-lg hover:opacity-80 flex items-center justify-center"
                style={{ backgroundColor: 'var(--bg-tertiary)', border: '1px solid var(--border)' }}
                title={`Y+${jogDistance}`}
              >
                <ArrowUp size={16} style={{ color: 'var(--text-primary)' }} />
              </button>
              <div />
              <button
                onClick={() => handleJog(-jogDistance, 0, 0)}
                className="p-2 rounded-lg hover:opacity-80 flex items-center justify-center"
                style={{ backgroundColor: 'var(--bg-tertiary)', border: '1px solid var(--border)' }}
                title={`X-${jogDistance}`}
              >
                <ArrowLeft size={16} style={{ color: 'var(--text-primary)' }} />
              </button>
              <button
                onClick={() => handleHome()}
                className="p-2 rounded-lg hover:opacity-80 flex items-center justify-center"
                style={{ backgroundColor: 'var(--accent-alpha)', border: '1px solid var(--accent)' }}
                title="Home"
              >
                <Home size={16} style={{ color: 'var(--accent)' }} />
              </button>
              <button
                onClick={() => handleJog(jogDistance, 0, 0)}
                className="p-2 rounded-lg hover:opacity-80 flex items-center justify-center"
                style={{ backgroundColor: 'var(--bg-tertiary)', border: '1px solid var(--border)' }}
                title={`X+${jogDistance}`}
              >
                <ArrowRight size={16} style={{ color: 'var(--text-primary)' }} />
              </button>
              <div />
              <button
                onClick={() => handleJog(0, -jogDistance, 0)}
                className="p-2 rounded-lg hover:opacity-80 flex items-center justify-center"
                style={{ backgroundColor: 'var(--bg-tertiary)', border: '1px solid var(--border)' }}
                title={`Y-${jogDistance}`}
              >
                <ArrowDown size={16} style={{ color: 'var(--text-primary)' }} />
              </button>
              <div />
            </div>

            {/* Z controls */}
            <div className="flex flex-col gap-1 items-center">
              <span className="text-xs mb-1" style={{ color: 'var(--text-secondary)' }}>Z</span>
              <button
                onClick={() => handleJog(0, 0, jogDistance)}
                className="p-2 rounded-lg hover:opacity-80 flex items-center justify-center"
                style={{ backgroundColor: 'var(--bg-tertiary)', border: '1px solid var(--border)', width: 38 }}
                title={`Z+${jogDistance}`}
              >
                <ArrowUp size={16} style={{ color: 'var(--text-primary)' }} />
              </button>
              <button
                onClick={() => handleJog(0, 0, -jogDistance)}
                className="p-2 rounded-lg hover:opacity-80 flex items-center justify-center"
                style={{ backgroundColor: 'var(--bg-tertiary)', border: '1px solid var(--border)', width: 38 }}
                title={`Z-${jogDistance}`}
              >
                <ArrowDown size={16} style={{ color: 'var(--text-primary)' }} />
              </button>
            </div>

            {/* Distance selector */}
            <div className="flex flex-col gap-1">
              <span className="text-xs mb-1" style={{ color: 'var(--text-secondary)' }}>Distancia</span>
              {[0.1, 1, 10, 100].map(d => (
                <button
                  key={d}
                  onClick={() => setJogDistance(d)}
                  className="px-3 py-1 rounded text-xs font-mono font-medium hover:opacity-90"
                  style={{
                    backgroundColor: jogDistance === d ? 'var(--accent)' : 'var(--bg-tertiary)',
                    color: jogDistance === d ? '#ffffff' : 'var(--text-primary)',
                    border: `1px solid ${jogDistance === d ? 'var(--accent)' : 'var(--border)'}`,
                  }}
                >
                  {d}mm
                </button>
              ))}
            </div>
          </div>
        </div>

        {/* G-code manual */}
        <div>
          <h4 className="text-xs font-semibold mb-2 flex items-center gap-1.5" style={{ color: 'var(--text-secondary)' }}>
            <Send size={14} /> G-code manual
          </h4>
          <div className="flex items-center gap-2">
            <input
              type="text"
              value={gcodeInput}
              onChange={(e) => setGcodeInput(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter') handleSendGcode()
              }}
              placeholder="G28, M104 S200, etc."
              className="flex-1 px-3 py-1.5 rounded-lg text-xs outline-none font-mono"
              style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
            />
            <button
              onClick={() => handleSendGcode()}
              className="flex items-center gap-1 px-3 py-1.5 rounded-lg text-xs font-medium hover:opacity-90"
              style={{ backgroundColor: 'var(--accent)', color: '#ffffff' }}
            >
              <Send size={12} /> Enviar
            </button>
          </div>
        </div>

        {/* Archivos en la impresora */}
        <div>
          <div className="flex items-center justify-between mb-2">
            <h4 className="text-xs font-semibold flex items-center gap-1.5" style={{ color: 'var(--text-secondary)' }}>
              <FileText size={14} /> Archivos en la impresora
            </h4>
            <button
              onClick={() => onLoadFiles()}
              disabled={loadingFiles}
              className="text-xs flex items-center gap-1 hover:opacity-80"
              style={{ color: 'var(--accent)' }}
            >
              {loadingFiles ? <Loader2 size={12} className="animate-spin" /> : <RefreshCw size={12} />}
              Refrescar
            </button>
          </div>

          {files.length === 0 ? (
            <p className="text-xs py-2" style={{ color: 'var(--text-secondary)' }}>
              {loadingFiles ? 'Cargando...' : 'No hay archivos'}
            </p>
          ) : (
            <div className="rounded-lg overflow-hidden" style={{ border: '1px solid var(--border)' }}>
              {files.map((file, i) => (
                <div
                  key={file.name}
                  className="flex items-center justify-between px-3 py-2 text-xs"
                  style={{
                    borderTop: i > 0 ? '1px solid var(--border)' : undefined,
                    backgroundColor: i % 2 === 0 ? 'var(--bg-tertiary)' : 'transparent',
                  }}
                >
                  <div className="flex items-center gap-2 min-w-0 flex-1">
                    <FileText size={12} style={{ color: 'var(--text-secondary)', flexShrink: 0 }} />
                    <span className="truncate font-mono" style={{ color: 'var(--text-primary)' }}>{file.name}</span>
                    <span style={{ color: 'var(--text-secondary)', flexShrink: 0 }}>{formatFileSize(file.size)}</span>
                  </div>
                  <div className="flex items-center gap-1 ml-2">
                    <button
                      onClick={() => handlePrintFile(file.name)}
                      className="p-1 rounded hover:opacity-80"
                      style={{ color: 'var(--success)' }}
                      title="Imprimir"
                    >
                      <Play size={14} />
                    </button>
                    <button
                      onClick={() => handleDeleteFile(file.name)}
                      className="p-1 rounded hover:opacity-80"
                      style={{ color: 'var(--danger)' }}
                      title="Eliminar"
                    >
                      <Trash2 size={14} />
                    </button>
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>
      </div>
  )
}

import { useEffect, useState, useCallback } from 'react'
import {
  Thermometer, Wifi, Radio, Battery, BatteryLow, BatteryMedium, BatteryFull, BatteryWarning,
  Signal, SignalLow, SignalMedium, SignalHigh, Plus, Trash2, Check, X, Loader2,
  AlertTriangle, Bell, BellOff, Settings2, Pencil, ChevronDown, ChevronUp, Usb,
} from 'lucide-react'
import {
  fetchSensorLatest, fetchSensorAlerts, fetchReceiverStatus,
  updateSensorDeviceStatus, updateSensorDeviceName, deleteSensorDevice,
  registerSensorDevice, createSensorAlert, updateSensorAlert, deleteSensorAlert,
  configureReceiver, fetchSensorReadings,
} from '../api'
import { useAuth } from '../auth/AuthContext'
import type { SensorLatest, SensorAlert, ReceiverStatus, SensorReading } from '../types'

const READING_LABELS: Record<string, string> = {
  temperature: 'Temperatura', humidity: 'Humedad', pressure: 'Presion',
  co2: 'CO2', tvoc: 'TVOC', pm2_5: 'PM2.5', pm10: 'PM10',
  light: 'Luz', uv_index: 'UV', noise: 'Ruido',
  soil_moisture: 'Humedad suelo', water_level: 'Nivel agua', ph: 'pH',
  conductivity: 'Conductividad', dissolved_o2: 'O2 disuelto',
  wind_speed: 'Viento', wind_direction: 'Dir. viento', rain: 'Lluvia',
  weight: 'Peso', current: 'Corriente', voltage: 'Voltaje',
  power: 'Potencia', energy: 'Energia', distance: 'Distancia', flow_rate: 'Flujo',
}

const READING_UNITS: Record<string, string> = {
  temperature: '°C', humidity: '%', pressure: 'hPa', co2: 'ppm', tvoc: 'ppb',
  pm2_5: 'ug/m3', pm10: 'ug/m3', light: 'lux', noise: 'dB',
  soil_moisture: '%', water_level: 'cm', conductivity: 'uS/cm', dissolved_o2: 'mg/L',
  wind_speed: 'm/s', wind_direction: '°', rain: 'mm', weight: 'g',
  current: 'A', voltage: 'V', power: 'W', energy: 'Wh', distance: 'cm', flow_rate: 'L/min',
}

function timeAgo(iso: string): string {
  const diff = Date.now() - new Date(iso).getTime()
  const mins = Math.floor(diff / 60000)
  if (mins < 1) return 'ahora'
  if (mins < 60) return `hace ${mins}m`
  const hrs = Math.floor(mins / 60)
  if (hrs < 24) return `hace ${hrs}h`
  return `hace ${Math.floor(hrs / 24)}d`
}

function BatteryIcon({ level }: { level: number | null }) {
  if (level === null) return <Battery size={16} style={{ color: 'var(--text-secondary)' }} />
  if (level <= 10) return <BatteryWarning size={16} style={{ color: 'var(--danger)' }} />
  if (level <= 30) return <BatteryLow size={16} style={{ color: 'var(--warning)' }} />
  if (level <= 70) return <BatteryMedium size={16} style={{ color: 'var(--text-secondary)' }} />
  return <BatteryFull size={16} style={{ color: 'var(--success)' }} />
}

function SignalIcon({ rssi }: { rssi: number | null }) {
  if (rssi === null) return <Signal size={16} style={{ color: 'var(--text-secondary)' }} />
  if (rssi > -50) return <SignalHigh size={16} style={{ color: 'var(--success)' }} />
  if (rssi > -70) return <SignalMedium size={16} style={{ color: 'var(--warning)' }} />
  return <SignalLow size={16} style={{ color: 'var(--danger)' }} />
}

// SVG mini-chart
function MiniChart({ data, width = 200, height = 60 }: { data: number[]; width?: number; height?: number }) {
  if (data.length < 2) return null
  const min = Math.min(...data)
  const max = Math.max(...data)
  const range = max - min || 1
  const pad = 4
  const w = width - pad * 2
  const h = height - pad * 2

  const points = data.map((v, i) => {
    const x = pad + (i / (data.length - 1)) * w
    const y = pad + h - ((v - min) / range) * h
    return `${x},${y}`
  }).join(' ')

  return (
    <svg width={width} height={height} style={{ display: 'block' }}>
      <polyline
        points={points}
        fill="none"
        stroke="var(--accent)"
        strokeWidth="1.5"
        strokeLinejoin="round"
      />
    </svg>
  )
}

export default function SensorsPage() {
  const { isAdmin } = useAuth()
  const [latest, setLatest] = useState<SensorLatest[]>([])
  const [alerts, setAlerts] = useState<SensorAlert[]>([])
  const [receiver, setReceiver] = useState<ReceiverStatus>({ connected: false, port: null, error: null })
  const [loading, setLoading] = useState(true)
  const [expanded, setExpanded] = useState<string | null>(null)
  const [editingName, setEditingName] = useState<string | null>(null)
  const [nameInput, setNameInput] = useState('')
  const [showAddDevice, setShowAddDevice] = useState(false)
  const [addForm, setAddForm] = useState({ name: '', mac: '', device_type: '', connection: 'wifi' })
  const [showAddAlert, setShowAddAlert] = useState(false)
  const [alertForm, setAlertForm] = useState({ device_id: '', key: '', condition: 'above', threshold: 0, cooldown_minutes: 30 })
  const [portInput, setPortInput] = useState('')
  const [showPortConfig, setShowPortConfig] = useState(false)
  const [chartData, setChartData] = useState<Record<string, SensorReading[]>>({})
  const [chartKey, setChartKey] = useState<Record<string, string>>({})
  const [chartRange, setChartRange] = useState<Record<string, string>>({})

  const loadData = useCallback(async () => {
    try {
      const [l, a, r] = await Promise.all([
        fetchSensorLatest(),
        fetchSensorAlerts(),
        fetchReceiverStatus(),
      ])
      setLatest(l)
      setAlerts(a)
      setReceiver(r)
      if (r.port) setPortInput(r.port)
    } catch {} finally { setLoading(false) }
  }, [])

  useEffect(() => { loadData() }, [loadData])

  // Polling cada 10s
  useEffect(() => {
    const interval = setInterval(async () => {
      try {
        const [l, r] = await Promise.all([fetchSensorLatest(), fetchReceiverStatus()])
        setLatest(l)
        setReceiver(r)
      } catch {}
    }, 10000)
    return () => clearInterval(interval)
  }, [])

  async function loadChart(deviceId: string, key: string, range: string) {
    const now = new Date()
    let from: Date
    switch (range) {
      case '1h': from = new Date(now.getTime() - 3600000); break
      case '6h': from = new Date(now.getTime() - 6 * 3600000); break
      case '24h': from = new Date(now.getTime() - 24 * 3600000); break
      case '7d': from = new Date(now.getTime() - 7 * 86400000); break
      default: from = new Date(now.getTime() - 24 * 3600000)
    }
    try {
      const readings = await fetchSensorReadings(deviceId, { key, from: from.toISOString(), limit: 500 })
      setChartData(prev => ({ ...prev, [deviceId]: readings.reverse() }))
    } catch {}
    setChartKey(prev => ({ ...prev, [deviceId]: key }))
    setChartRange(prev => ({ ...prev, [deviceId]: range }))
  }

  async function handleAccept(id: string) {
    try { await updateSensorDeviceStatus(id, 'accepted'); await loadData() } catch {}
  }
  async function handleReject(id: string) {
    try { await updateSensorDeviceStatus(id, 'rejected'); await loadData() } catch {}
  }
  async function handleDelete(id: string) {
    if (!confirm('Eliminar dispositivo y sus lecturas?')) return
    try { await deleteSensorDevice(id); await loadData() } catch {}
  }
  async function handleSaveName(id: string) {
    try { await updateSensorDeviceName(id, nameInput); setEditingName(null); await loadData() } catch {}
  }
  async function handleAddDevice() {
    if (!addForm.name.trim() || !addForm.mac.trim()) return
    try { await registerSensorDevice(addForm); setShowAddDevice(false); setAddForm({ name: '', mac: '', device_type: '', connection: 'wifi' }); await loadData() } catch (e: any) { alert(e.message) }
  }
  async function handleAddAlert() {
    if (!alertForm.device_id || !alertForm.key) return
    try { await createSensorAlert(alertForm); setShowAddAlert(false); await loadData() } catch (e: any) { alert(e.message) }
  }
  async function handleToggleAlert(id: string, enabled: boolean) {
    try { await updateSensorAlert(id, { enabled }); await loadData() } catch {}
  }
  async function handleDeleteAlert(id: string) {
    try { await deleteSensorAlert(id); await loadData() } catch {}
  }
  async function handleConfigPort() {
    if (!portInput.trim()) return
    try { await configureReceiver(portInput.trim()); setShowPortConfig(false); await loadData() } catch (e: any) { alert(e.message) }
  }

  const pending = latest.filter(s => s.device?.status === 'pending')
  const accepted = latest.filter(s => s.device?.status === 'accepted')
  const rejected = latest.filter(s => s.device?.status === 'rejected')

  if (loading) {
    return (
      <div className="flex items-center justify-center h-64">
        <Loader2 className="animate-spin" size={32} style={{ color: 'var(--accent)' }} />
      </div>
    )
  }

  return (
    <div className="space-y-6">
      {/* Receptor status + acciones */}
      <div className="flex items-center justify-between flex-wrap gap-3">
        <div className="flex items-center gap-3">
          <div className="flex items-center gap-2" style={{ color: receiver.connected ? 'var(--success)' : 'var(--text-secondary)' }}>
            <Usb size={18} />
            <span className="text-sm font-medium">
              {receiver.connected ? `Receptor: ${receiver.port}` : receiver.port ? `Desconectado (${receiver.port})` : 'Sin receptor configurado'}
            </span>
            <span
              className="inline-block w-2 h-2 rounded-full"
              style={{ backgroundColor: receiver.connected ? 'var(--success)' : 'var(--danger)' }}
            />
          </div>
          {receiver.error && !receiver.connected && (
            <span className="text-xs" style={{ color: 'var(--danger)' }}>{receiver.error}</span>
          )}
        </div>
        <div className="flex items-center gap-2">
          {isAdmin && (
            <>
              <button
                onClick={() => setShowPortConfig(!showPortConfig)}
                className="px-3 py-1.5 rounded-lg text-sm flex items-center gap-1.5 transition-colors"
                style={{ backgroundColor: 'var(--bg-tertiary)', color: 'var(--text-primary)' }}
              >
                <Settings2 size={14} /> Puerto serial
              </button>
              <button
                onClick={() => setShowAddDevice(!showAddDevice)}
                className="px-3 py-1.5 rounded-lg text-sm flex items-center gap-1.5 transition-colors"
                style={{ backgroundColor: 'var(--accent)', color: '#fff' }}
              >
                <Plus size={14} /> Registrar WiFi
              </button>
            </>
          )}
        </div>
      </div>

      {/* Config puerto serial */}
      {showPortConfig && isAdmin && (
        <div className="p-4 rounded-xl" style={{ backgroundColor: 'var(--bg-secondary)', border: '1px solid var(--border)' }}>
          <div className="flex items-center gap-2">
            <input
              value={portInput}
              onChange={e => setPortInput(e.target.value)}
              placeholder="/dev/ttyUSB0"
              className="flex-1 px-3 py-2 rounded-lg text-sm"
              style={{ backgroundColor: 'var(--bg-tertiary)', color: 'var(--text-primary)', border: '1px solid var(--border)' }}
            />
            <button onClick={handleConfigPort} className="px-4 py-2 rounded-lg text-sm" style={{ backgroundColor: 'var(--accent)', color: '#fff' }}>
              Guardar
            </button>
          </div>
        </div>
      )}

      {/* Formulario agregar dispositivo WiFi */}
      {showAddDevice && isAdmin && (
        <div className="p-4 rounded-xl space-y-3" style={{ backgroundColor: 'var(--bg-secondary)', border: '1px solid var(--border)' }}>
          <div className="grid grid-cols-2 gap-3">
            <input value={addForm.name} onChange={e => setAddForm(p => ({ ...p, name: e.target.value }))} placeholder="Nombre" className="px-3 py-2 rounded-lg text-sm" style={{ backgroundColor: 'var(--bg-tertiary)', color: 'var(--text-primary)', border: '1px solid var(--border)' }} />
            <input value={addForm.mac} onChange={e => setAddForm(p => ({ ...p, mac: e.target.value }))} placeholder="MAC (AA:BB:CC:DD:EE:FF)" className="px-3 py-2 rounded-lg text-sm" style={{ backgroundColor: 'var(--bg-tertiary)', color: 'var(--text-primary)', border: '1px solid var(--border)' }} />
            <input value={addForm.device_type} onChange={e => setAddForm(p => ({ ...p, device_type: e.target.value }))} placeholder="Tipo (ej: DHT22, BME280)" className="px-3 py-2 rounded-lg text-sm" style={{ backgroundColor: 'var(--bg-tertiary)', color: 'var(--text-primary)', border: '1px solid var(--border)' }} />
            <select value={addForm.connection} onChange={e => setAddForm(p => ({ ...p, connection: e.target.value }))} className="px-3 py-2 rounded-lg text-sm" style={{ backgroundColor: 'var(--bg-tertiary)', color: 'var(--text-primary)', border: '1px solid var(--border)' }}>
              <option value="wifi">WiFi</option>
              <option value="espnow">ESP-NOW</option>
            </select>
          </div>
          <div className="flex justify-end gap-2">
            <button onClick={() => setShowAddDevice(false)} className="px-3 py-1.5 rounded-lg text-sm" style={{ color: 'var(--text-secondary)' }}>Cancelar</button>
            <button onClick={handleAddDevice} className="px-4 py-1.5 rounded-lg text-sm" style={{ backgroundColor: 'var(--accent)', color: '#fff' }}>Registrar</button>
          </div>
        </div>
      )}

      {/* Dispositivos pendientes */}
      {pending.length > 0 && isAdmin && (
        <div className="space-y-2">
          <h3 className="text-sm font-semibold flex items-center gap-2" style={{ color: 'var(--warning)' }}>
            <AlertTriangle size={16} /> Pendientes de aprobacion ({pending.length})
          </h3>
          <div className="grid gap-3 grid-cols-1 md:grid-cols-2 xl:grid-cols-3">
            {pending.map(s => s.device && (
              <div key={s.device.id} className="p-4 rounded-xl" style={{ backgroundColor: 'var(--bg-secondary)', border: '2px solid var(--warning)' }}>
                <div className="flex items-center justify-between mb-2">
                  <span className="text-xs font-mono" style={{ color: 'var(--text-secondary)' }}>{s.device.mac}</span>
                  <div className="flex gap-1">
                    <button onClick={() => handleAccept(s.device!.id)} className="p-1.5 rounded-lg" style={{ backgroundColor: 'var(--success)', color: '#fff' }} title="Aceptar">
                      <Check size={14} />
                    </button>
                    <button onClick={() => handleReject(s.device!.id)} className="p-1.5 rounded-lg" style={{ backgroundColor: 'var(--danger)', color: '#fff' }} title="Rechazar">
                      <X size={14} />
                    </button>
                  </div>
                </div>
                {Object.entries(s.readings).length > 0 && (
                  <div className="flex flex-wrap gap-2 mt-2">
                    {Object.entries(s.readings).map(([k, v]) => (
                      <span key={k} className="text-xs px-2 py-0.5 rounded" style={{ backgroundColor: 'var(--bg-tertiary)', color: 'var(--text-primary)' }}>
                        {READING_LABELS[k] || k}: {typeof v === 'number' ? v.toFixed(1) : v}{READING_UNITS[k] || ''}
                      </span>
                    ))}
                  </div>
                )}
              </div>
            ))}
          </div>
        </div>
      )}

      {/* Dispositivos aceptados */}
      {accepted.length === 0 && pending.length === 0 ? (
        <div className="text-center py-16" style={{ color: 'var(--text-secondary)' }}>
          <Thermometer size={48} className="mx-auto mb-4 opacity-30" />
          <p>No hay sensores registrados</p>
          <p className="text-sm mt-1">Los sensores se registran automaticamente al enviar datos</p>
        </div>
      ) : (
        <div className="grid gap-4 grid-cols-1 md:grid-cols-2 xl:grid-cols-3">
          {accepted.map(s => {
            if (!s.device) return null
            const d = s.device
            const isExpanded = expanded === d.id
            const deviceAlerts = alerts.filter(a => a.device_id === d.id)
            const readingKeys = Object.keys(s.readings)
            const currentChartKey = chartKey[d.id] || readingKeys[0]
            const currentRange = chartRange[d.id] || '24h'
            const currentChartData = chartData[d.id]

            return (
              <div
                key={d.id}
                className="rounded-xl overflow-hidden transition-all"
                style={{ backgroundColor: 'var(--bg-secondary)', border: `1px solid ${isExpanded ? 'var(--accent)' : 'var(--border)'}` }}
              >
                {/* Header */}
                <div className="p-4">
                  <div className="flex items-center justify-between mb-3">
                    <div className="flex items-center gap-2 min-w-0">
                      <span
                        className="w-2.5 h-2.5 rounded-full flex-shrink-0"
                        style={{ backgroundColor: s.online ? 'var(--success)' : 'var(--text-secondary)' }}
                      />
                      {editingName === d.id ? (
                        <div className="flex items-center gap-1">
                          <input
                            value={nameInput}
                            onChange={e => setNameInput(e.target.value)}
                            className="px-2 py-0.5 rounded text-sm"
                            style={{ backgroundColor: 'var(--bg-tertiary)', color: 'var(--text-primary)', border: '1px solid var(--border)', width: '120px' }}
                            autoFocus
                            onKeyDown={e => e.key === 'Enter' && handleSaveName(d.id)}
                          />
                          <button onClick={() => handleSaveName(d.id)} className="p-0.5"><Check size={14} style={{ color: 'var(--success)' }} /></button>
                          <button onClick={() => setEditingName(null)} className="p-0.5"><X size={14} style={{ color: 'var(--text-secondary)' }} /></button>
                        </div>
                      ) : (
                        <span className="font-medium truncate" style={{ color: 'var(--text-primary)' }}>
                          {d.name || d.mac}
                        </span>
                      )}
                    </div>
                    <div className="flex items-center gap-2">
                      <span className="text-xs px-1.5 py-0.5 rounded" style={{
                        backgroundColor: d.connection === 'espnow' ? 'var(--accent)' + '22' : 'var(--bg-tertiary)',
                        color: d.connection === 'espnow' ? 'var(--accent)' : 'var(--text-secondary)',
                      }}>
                        {d.connection === 'espnow' ? <Radio size={10} className="inline mr-1" /> : <Wifi size={10} className="inline mr-1" />}
                        {d.connection.toUpperCase()}
                      </span>
                    </div>
                  </div>

                  {/* Readings */}
                  <div className="space-y-1.5">
                    {Object.entries(s.readings).map(([k, v]) => (
                      <div key={k} className="flex items-center justify-between">
                        <span className="text-sm" style={{ color: 'var(--text-secondary)' }}>{READING_LABELS[k] || k}</span>
                        <span className="text-sm font-medium tabular-nums" style={{ color: 'var(--text-primary)' }}>
                          {typeof v === 'number' ? v.toFixed(1) : v}
                          <span className="text-xs ml-0.5" style={{ color: 'var(--text-secondary)' }}>{READING_UNITS[k] || ''}</span>
                        </span>
                      </div>
                    ))}
                    {readingKeys.length === 0 && (
                      <span className="text-xs" style={{ color: 'var(--text-secondary)' }}>Sin datos</span>
                    )}
                  </div>

                  {/* Footer: battery, signal, last seen */}
                  <div className="flex items-center justify-between mt-3 pt-2" style={{ borderTop: '1px solid var(--border)' }}>
                    <div className="flex items-center gap-3">
                      <div className="flex items-center gap-1" title={d.battery !== null ? `${d.battery}%` : 'Sin bateria'}>
                        <BatteryIcon level={d.battery} />
                        {d.battery !== null && <span className="text-xs" style={{ color: 'var(--text-secondary)' }}>{d.battery}%</span>}
                      </div>
                      {d.connection === 'espnow' && (
                        <div className="flex items-center gap-1" title={d.rssi !== null ? `${d.rssi} dBm` : ''}>
                          <SignalIcon rssi={d.rssi} />
                          {d.rssi !== null && <span className="text-xs" style={{ color: 'var(--text-secondary)' }}>{d.rssi}</span>}
                        </div>
                      )}
                    </div>
                    <div className="flex items-center gap-2">
                      {d.last_seen && (
                        <span className="text-xs" style={{ color: 'var(--text-secondary)' }}>{timeAgo(d.last_seen)}</span>
                      )}
                      <button
                        onClick={() => {
                          if (isExpanded) { setExpanded(null) } else {
                            setExpanded(d.id)
                            if (readingKeys.length > 0) loadChart(d.id, readingKeys[0], '24h')
                          }
                        }}
                        className="p-1 rounded"
                        style={{ color: 'var(--text-secondary)' }}
                      >
                        {isExpanded ? <ChevronUp size={16} /> : <ChevronDown size={16} />}
                      </button>
                    </div>
                  </div>
                </div>

                {/* Expanded: chart + alerts + actions */}
                {isExpanded && (
                  <div className="px-4 pb-4 space-y-3" style={{ borderTop: '1px solid var(--border)' }}>
                    {/* Info */}
                    <div className="flex items-center justify-between pt-3">
                      <span className="text-xs font-mono" style={{ color: 'var(--text-secondary)' }}>{d.mac}</span>
                      {d.device_type && <span className="text-xs" style={{ color: 'var(--text-secondary)' }}>{d.device_type}</span>}
                    </div>

                    {/* Chart controls */}
                    {readingKeys.length > 0 && (
                      <>
                        <div className="flex items-center gap-2 flex-wrap">
                          <select
                            value={currentChartKey}
                            onChange={e => loadChart(d.id, e.target.value, currentRange)}
                            className="px-2 py-1 rounded text-xs"
                            style={{ backgroundColor: 'var(--bg-tertiary)', color: 'var(--text-primary)', border: '1px solid var(--border)' }}
                          >
                            {readingKeys.map(k => (
                              <option key={k} value={k}>{READING_LABELS[k] || k}</option>
                            ))}
                          </select>
                          {['1h', '6h', '24h', '7d'].map(r => (
                            <button
                              key={r}
                              onClick={() => loadChart(d.id, currentChartKey, r)}
                              className="px-2 py-1 rounded text-xs"
                              style={{
                                backgroundColor: currentRange === r ? 'var(--accent)' : 'var(--bg-tertiary)',
                                color: currentRange === r ? '#fff' : 'var(--text-secondary)',
                              }}
                            >
                              {r}
                            </button>
                          ))}
                        </div>
                        {currentChartData && currentChartData.length > 1 ? (
                          <MiniChart data={currentChartData.map(r => r.value)} width={300} height={80} />
                        ) : (
                          <span className="text-xs" style={{ color: 'var(--text-secondary)' }}>Sin datos para este rango</span>
                        )}
                      </>
                    )}

                    {/* Alerts for this device */}
                    {deviceAlerts.length > 0 && (
                      <div className="space-y-1">
                        <span className="text-xs font-semibold" style={{ color: 'var(--text-secondary)' }}>Alertas</span>
                        {deviceAlerts.map(a => (
                          <div key={a.id} className="flex items-center justify-between text-xs py-1 px-2 rounded" style={{ backgroundColor: 'var(--bg-tertiary)' }}>
                            <span style={{ color: 'var(--text-primary)' }}>
                              {READING_LABELS[a.key] || a.key} {a.condition === 'above' ? '>' : '<'} {a.threshold}
                            </span>
                            <div className="flex items-center gap-1">
                              <button onClick={() => handleToggleAlert(a.id, !a.enabled)} className="p-0.5" title={a.enabled ? 'Desactivar' : 'Activar'}>
                                {a.enabled ? <Bell size={12} style={{ color: 'var(--accent)' }} /> : <BellOff size={12} style={{ color: 'var(--text-secondary)' }} />}
                              </button>
                              {isAdmin && (
                                <button onClick={() => handleDeleteAlert(a.id)} className="p-0.5">
                                  <Trash2 size={12} style={{ color: 'var(--danger)' }} />
                                </button>
                              )}
                            </div>
                          </div>
                        ))}
                      </div>
                    )}

                    {/* Actions */}
                    {isAdmin && (
                      <div className="flex items-center gap-2 pt-1">
                        <button
                          onClick={() => { setEditingName(d.id); setNameInput(d.name) }}
                          className="px-2 py-1 rounded text-xs flex items-center gap-1"
                          style={{ backgroundColor: 'var(--bg-tertiary)', color: 'var(--text-primary)' }}
                        >
                          <Pencil size={12} /> Renombrar
                        </button>
                        <button
                          onClick={() => { setShowAddAlert(true); setAlertForm(f => ({ ...f, device_id: d.id, key: readingKeys[0] || '' })) }}
                          className="px-2 py-1 rounded text-xs flex items-center gap-1"
                          style={{ backgroundColor: 'var(--bg-tertiary)', color: 'var(--text-primary)' }}
                        >
                          <Bell size={12} /> Alerta
                        </button>
                        <button
                          onClick={() => handleDelete(d.id)}
                          className="px-2 py-1 rounded text-xs flex items-center gap-1 ml-auto"
                          style={{ color: 'var(--danger)' }}
                        >
                          <Trash2 size={12} /> Eliminar
                        </button>
                      </div>
                    )}
                  </div>
                )}
              </div>
            )
          })}
        </div>
      )}

      {/* Rechazados (colapsable) */}
      {rejected.length > 0 && isAdmin && (
        <details className="mt-4">
          <summary className="text-sm cursor-pointer" style={{ color: 'var(--text-secondary)' }}>
            Rechazados ({rejected.length})
          </summary>
          <div className="mt-2 grid gap-2 grid-cols-1 md:grid-cols-2 xl:grid-cols-3">
            {rejected.map(s => s.device && (
              <div key={s.device.id} className="p-3 rounded-xl flex items-center justify-between" style={{ backgroundColor: 'var(--bg-secondary)', border: '1px solid var(--border)', opacity: 0.6 }}>
                <span className="text-sm font-mono" style={{ color: 'var(--text-secondary)' }}>{s.device.mac}</span>
                <div className="flex gap-1">
                  <button onClick={() => handleAccept(s.device!.id)} className="p-1 rounded" title="Aceptar">
                    <Check size={14} style={{ color: 'var(--success)' }} />
                  </button>
                  <button onClick={() => handleDelete(s.device!.id)} className="p-1 rounded" title="Eliminar">
                    <Trash2 size={14} style={{ color: 'var(--danger)' }} />
                  </button>
                </div>
              </div>
            ))}
          </div>
        </details>
      )}

      {/* Modal crear alerta */}
      {showAddAlert && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40" onClick={() => setShowAddAlert(false)}>
          <div className="w-full max-w-md p-6 rounded-2xl space-y-4" style={{ backgroundColor: 'var(--bg-secondary)' }} onClick={e => e.stopPropagation()}>
            <h3 className="font-semibold" style={{ color: 'var(--text-primary)' }}>Nueva alerta</h3>
            <div className="space-y-3">
              <select
                value={alertForm.device_id}
                onChange={e => {
                  const dev = accepted.find(s => s.device?.id === e.target.value)
                  const keys = dev ? Object.keys(dev.readings) : []
                  setAlertForm(f => ({ ...f, device_id: e.target.value, key: keys[0] || '' }))
                }}
                className="w-full px-3 py-2 rounded-lg text-sm"
                style={{ backgroundColor: 'var(--bg-tertiary)', color: 'var(--text-primary)', border: '1px solid var(--border)' }}
              >
                {accepted.map(s => s.device && (
                  <option key={s.device.id} value={s.device.id}>{s.device.name || s.device.mac}</option>
                ))}
              </select>
              <div className="grid grid-cols-3 gap-2">
                <select value={alertForm.key} onChange={e => setAlertForm(f => ({ ...f, key: e.target.value }))} className="px-3 py-2 rounded-lg text-sm" style={{ backgroundColor: 'var(--bg-tertiary)', color: 'var(--text-primary)', border: '1px solid var(--border)' }}>
                  {(() => {
                    const dev = accepted.find(s => s.device?.id === alertForm.device_id)
                    return dev ? Object.keys(dev.readings).map(k => (
                      <option key={k} value={k}>{READING_LABELS[k] || k}</option>
                    )) : null
                  })()}
                </select>
                <select value={alertForm.condition} onChange={e => setAlertForm(f => ({ ...f, condition: e.target.value }))} className="px-3 py-2 rounded-lg text-sm" style={{ backgroundColor: 'var(--bg-tertiary)', color: 'var(--text-primary)', border: '1px solid var(--border)' }}>
                  <option value="above">Mayor que</option>
                  <option value="below">Menor que</option>
                </select>
                <input type="number" step="any" value={alertForm.threshold} onChange={e => setAlertForm(f => ({ ...f, threshold: parseFloat(e.target.value) || 0 }))} placeholder="Umbral" className="px-3 py-2 rounded-lg text-sm" style={{ backgroundColor: 'var(--bg-tertiary)', color: 'var(--text-primary)', border: '1px solid var(--border)' }} />
              </div>
              <div className="flex items-center gap-2">
                <span className="text-sm" style={{ color: 'var(--text-secondary)' }}>Cooldown (min):</span>
                <input type="number" value={alertForm.cooldown_minutes} onChange={e => setAlertForm(f => ({ ...f, cooldown_minutes: parseInt(e.target.value) || 30 }))} className="w-20 px-2 py-1 rounded-lg text-sm" style={{ backgroundColor: 'var(--bg-tertiary)', color: 'var(--text-primary)', border: '1px solid var(--border)' }} />
              </div>
            </div>
            <div className="flex justify-end gap-2">
              <button onClick={() => setShowAddAlert(false)} className="px-4 py-2 rounded-lg text-sm" style={{ color: 'var(--text-secondary)' }}>Cancelar</button>
              <button onClick={handleAddAlert} className="px-4 py-2 rounded-lg text-sm" style={{ backgroundColor: 'var(--accent)', color: '#fff' }}>Crear</button>
            </div>
          </div>
        </div>
      )}
    </div>
  )
}

import type { SensorReport } from '../api/sensors'

/** Abre una ventana con el informe de temperaturas lista para imprimir (certificaciones) */
export function printSensorReport(r: SensorReport) {
  const w = window.open('', '_blank', 'width=900,height=700')
  if (!w) return
  const esc = (s: string) => s.replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]!))
  const n = (v: number) => v.toFixed(1)
  const keys = r.keys.map((k) => `<tr><td>${esc(k.key)}</td><td>${n(k.min)} ${esc(k.unit)}</td><td>${n(k.max)} ${esc(k.unit)}</td>
    <td>${n(k.avg)} ${esc(k.unit)}</td><td>${k.count}</td><td>${esc(k.first_at)}</td><td>${esc(k.last_at)}</td></tr>`).join('')
  const alerts = r.alerts.map((a) => `<tr><td>${esc(a.key)} ${a.condition === 'above' ? '&gt;' : '&lt;'} ${a.threshold}${a.critical ? ' (critica)' : ''}</td>
    <td>${a.minutes_out_of_range} min</td><td>${a.readings_out_of_range}</td><td>${a.incidents}</td></tr>`).join('')
  w.document.write(`<!doctype html><html><head><meta charset="utf-8"><title>Informe ${esc(r.device_name)}</title>
    <style>body{font-family:system-ui,sans-serif;margin:32px;color:#111}h1{font-size:20px;margin:0}h2{font-size:15px;margin:24px 0 6px}
    p{color:#555;font-size:12px;margin:2px 0}table{border-collapse:collapse;width:100%;font-size:12px}
    th,td{border:1px solid #bbb;padding:5px 8px;text-align:left}th{background:#eee}</style></head>
    <body><h1>Informe de temperaturas: ${esc(r.device_name)}</h1>
    <p>Periodo: ${esc(r.from)} a ${esc(r.to)} | Lecturas: ${r.total_readings} | Generado: ${esc(r.generated_at)}</p>
    <h2>Resumen por magnitud</h2>
    <table><tr><th>Clave</th><th>Minimo</th><th>Maximo</th><th>Promedio</th><th>Lecturas</th><th>Primera</th><th>Ultima</th></tr>${keys || '<tr><td colspan="7">Sin lecturas en el periodo</td></tr>'}</table>
    <h2>Tiempo fuera de rango</h2>
    <table><tr><th>Alerta</th><th>Tiempo aprox.</th><th>Lecturas fuera</th><th>Incidentes</th></tr>${alerts || '<tr><td colspan="4">Sin alertas configuradas</td></tr>'}</table>
    <p style="margin-top:16px">El tiempo fuera de rango es aproximado: suma el intervalo hasta la lectura siguiente (tope 60 min por hueco).</p>
    <script>window.onload=function(){window.print()}</script></body></html>`)
  w.document.close()
}

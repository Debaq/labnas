import type { Certificate } from '../api/workshops'
import { certificateUrl } from '../api/workshops'

const esc = (s: string) => s.replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]!))

/** Solo se aceptan rutas http(s) o relativas para el logo (nada de javascript:) */
const safeUrl = (u: string) => (/^(https?:)?\/\//.test(u) || u.startsWith('/') ? u : '')

/** Abre el certificado listo para imprimir o guardar como PDF (Imprimir > Guardar como PDF) */
export function printCertificate(c: Certificate) {
  const w = window.open('', '_blank', 'width=900,height=680')
  if (!w) return
  const logo = safeUrl(c.lab.logo_url)
  const dates = c.end_date && c.end_date !== c.start_date ? `${c.start_date} al ${c.end_date}` : c.start_date
  const url = c.code ? certificateUrl(c.code) : ''
  w.document.write(`<!doctype html><html><head><meta charset="utf-8"><title>Certificado ${esc(c.workshop)}</title>
    <style>@page{size:A4 landscape;margin:0}
    body{font-family:Georgia,serif;margin:0;padding:40px;text-align:center;color:#222}
    .m{border:6px double #555;padding:40px 50px;min-height:480px;display:flex;flex-direction:column;justify-content:center;gap:10px}
    img{max-height:70px;margin:0 auto}h1{font-size:34px;letter-spacing:4px;margin:6px 0}
    h2{font-size:28px;margin:10px 0;border-bottom:1px solid #999;display:inline-block;padding:0 30px;align-self:center}
    p{margin:4px 0;font-size:16px}.s{font-size:12px;color:#555}</style></head><body><div class="m">
    ${logo ? `<img src="${esc(logo)}" alt="">` : ''}
    <p>${esc(c.lab.lab_name)}${c.lab.institution ? ` - ${esc(c.lab.institution)}` : ''}</p>
    <h1>CERTIFICADO</h1><p>Se certifica que</p><h2>${esc(c.name)}</h2>
    <p>asistio y aprobo el ${c.modality ? esc(c.modality) + ': ' : 'taller: '}<b>${esc(c.workshop)}</b></p>
    <p>${c.hours ? `Duracion: ${c.hours} horas. ` : ''}Fecha: ${esc(dates)}</p>
    <p>Asistencia: ${c.attended.length} de ${c.sessions.length} sesiones</p>
    <p class="s" style="margin-top:24px">Codigo de verificacion: <b>${esc(c.code ?? '')}</b><br>Verifica su autenticidad en ${esc(url)}</p>
    </div><script>window.onload=function(){window.print()}</script></body></html>`)
  w.document.close()
}

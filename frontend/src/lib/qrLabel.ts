/** Abre una ventana con una etiqueta QR lista para imprimir y pegar */
export function printQrLabel(qr: { svg: string }, title: string, subtitle: string, hint: string) {
  const w = window.open('', '_blank', 'width=420,height=560')
  if (!w) return
  const esc = (s: string) => s.replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]!))
  w.document.write(`<!doctype html><html><head><meta charset="utf-8"><title>${esc(title)}</title>
    <style>body{font-family:system-ui,sans-serif;text-align:center;margin:0;padding:24px}
    .l{display:inline-block;border:2px dashed #999;border-radius:12px;padding:16px 20px}
    svg{width:200px;height:200px}h1{font-size:18px;margin:8px 0 2px}p{margin:0;color:#555;font-size:12px}
    @media print{.l{border-color:#ccc}}</style></head>
    <body><div class="l">${qr.svg}<h1>${esc(title)}</h1>${subtitle ? `<p>${esc(subtitle)}</p>` : ''}
    <p style="margin-top:6px">${esc(hint)}</p></div>
    <script>window.onload=function(){window.print()}</script></body></html>`)
  w.document.close()
}

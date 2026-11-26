# Historial de sensores exportable

En la tarjeta expandida de cada sensor (Sensores), la sección **Exportar historial**
permite elegir un rango de fechas (por defecto el último mes) y:

- **Descargar CSV**: `GET /api/sensors/devices/{id}/export?from=AAAA-MM-DD&to=AAAA-MM-DD&format=csv`.
  Columnas: `fecha_hora_local,clave,valor,unidad`, en hora local del servidor, con BOM UTF-8
  para Excel.
- **Ver resumen**: `GET /api/sensors/devices/{id}/report?from=&to=`. Devuelve por clave
  mínimo, máximo, promedio, cantidad, primera y última lectura; y por cada alerta del
  sensor los minutos aproximados fuera de rango, las lecturas fuera de rango y los
  incidentes críticos abiertos en el periodo.
- **Imprimir informe**: abre una ventana con el resumen lista para imprimir o guardar
  como PDF, útil para auditorías y certificaciones.

## Detalles

- Rango máximo de 366 días; fechas inclusivas (de 00:00 del primer día al fin del último).
- Tiempo fuera de rango: para cada lectura fuera del umbral se suma el intervalo hasta la
  lectura siguiente, con tope de 60 minutos por hueco. Es una aproximación.
- Requiere nivel Ver del módulo Sensores. Cada exportación o informe queda en el registro
  de accesos («Exportó historial de …»).

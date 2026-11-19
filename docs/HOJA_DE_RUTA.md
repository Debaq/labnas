# Hoja de ruta de LabNAS

> Lluvia de ideas ordenada (noviembre 2026). Nada de esto está programado todavía:
> es la lista de dónde puede crecer LabNAS para un fablab / laboratorio universitario.
> Marcar `[x]` a medida que se implemente. Los números son estables: sirven para
> referirse a una idea aunque cambie de etapa.

## Contexto

LabNAS se usa en un **fablab universitario** (impresoras 3D, cortadora láser, CNC,
electrónica) que gana proyectos con fondos importantes. Una evaluación de Contraloría
dejó en evidencia que **no había nada escrito sobre cómo se movía el dinero**: el
trabajo era honesto, pero no se podía demostrar. Por eso la etapa 1 es administración
y transparencia, antes que cualquier función de fabricación.

## Principios para todo lo nuevo

1. **Trazabilidad por defecto.** Cada acción relevante deja registro de quién, qué y
   cuándo. Lo financiero va a una bitácora que no se puede editar ni borrar (idea 57).
2. **Tres niveles de acceso** en cada módulo:

   | Nivel | Quién lo ve | Ejemplos |
   |---|---|---|
   | 🔒 Privado | El equipo, según su rol | Datos personales, contratos, borradores |
   | 🤝 Con autorización | Quien tenga un permiso, con fecha de término | Expediente para Finanzas, un auditor o el financiador |
   | 🌐 Público | Cualquiera | Proyectos adjudicados, montos, bienes comprados, horas de uso, informes |

3. **La IA propone, una persona aprueba.** Igual que con el correo: nada se agenda,
   se registra ni se gasta solo.
4. **Separación de funciones.** Quien pide, quien aprueba y quien recibe deben ser
   personas distintas cuando hay dinero de por medio.
5. **Reutilizar lo que ya existe** (sensores, Telegram, calendario, tareas,
   impresión, cola 3D, passkeys, respaldos) antes de construir algo paralelo.
6. **Las reglas de seguridad de siempre:** sin root, middleware que niega por defecto,
   secretos cifrados en la base.
7. **Cada área nueva es un módulo** que se activa o desactiva en **Configuración >
   Módulos**, como los actuales (archivos, impresión, inventario, sensores...). Un
   módulo apagado desaparece del menú y el middleware bloquea sus rutas
   (`MODULE_ROUTE_PREFIXES`), incluidas las públicas. Módulos propuestos: `finanzas`
   (etapa 1), `transparencia`, `seguridad`, `maquinas`, `servicios`, `fabricacion`,
   `docencia`.

> ⚠️ LabNAS ordena y prueba los procesos; **no reemplaza** las normas oficiales.
> Antes de diseñar la etapa 1, revisar con Finanzas de la UACh y con las bases de cada
> financiador (ANID, gobierno regional, etc.). Para fondos públicos rige la
> Resolución N° 30 de 2015 de Contraloría sobre rendición de cuentas (confirmar con
> Finanzas si sigue vigente). La Ley 20.285 de Transparencia obliga a los órganos del
> Estado; la UACh es privada, así que la parte pública aplicaría por exigencia del
> financiador o como transparencia voluntaria.

---

## Etapa 0 · Base común

Lo que usan todas las demás etapas.

- [ ] **Personas y roles:** investigador responsable, administrativo, docente,
      estudiante, usuario externo, auditor (solo lectura y con fecha de término).
- [ ] **Niveles de acceso** privado / con autorización / público, aplicables a
      cualquier registro o archivo.
- [ ] **Catálogo único de máquinas y bienes con QR:** cada equipo tiene una ficha
      a la que llevan las ideas de reservas, mantenimiento, inventario y uso.
- [ ] **57. Bitácora que no se puede alterar.** Cada movimiento queda encadenado al
      anterior con su huella digital (hash). Nada se borra ni se edita: un error se
      corrige con un registro nuevo que lo anula. Si alguien adulterara un registro,
      la cadena lo delataría.
- [ ] **72. Registro de cada acceso:** quién vio o descargó qué y cuándo.

## Etapa 1 · Administración y transparencia ⭐ urgente

### Proyectos y compras
- [ ] **52. Proyecto con presupuesto aprobado por ítem** (personal, operación,
      equipos, viajes), fuente de financiamiento, fechas y responsable. *(absorbe la
      idea 15, «proyectos y presupuesto»)* Se apoya en el **módulo Portafolio** que ya
      existe (proyectos con institución, requisitos e hitos).
- [ ] **53. Flujo de compra con funciones separadas:** solicitud → justificación →
      cotizaciones → aprobación → orden de compra → recepción → factura → pago →
      rendición. El sistema no deja saltarse pasos.
- [ ] **54. Cotizaciones adjuntas** con cuadro comparativo y la razón de la elección.
- [ ] **55. Respaldos con lectura automática.** Se sube la factura y la IA extrae
      RUT, folio, monto y fecha, que una persona confirma. Las facturas electrónicas
      del SII vienen en XML, así que se pueden leer exactas.
- [ ] **56. Recepción conforme:** acta con foto del bien recibido, quién lo recibió
      y cuándo.
- [ ] **58. Aprobaciones firmadas con passkey** (ya existen en LabNAS): nadie puede
      decir «yo no aprobé eso».

### Inventario y presupuesto
- [ ] **59. Inventario de bienes por proyecto:** número de inventario, proyecto que
      lo pagó, ubicación, responsable y fotos. Amplía el **módulo Inventario** que ya
      existe (ya guarda costo, proveedor, ubicación, número de serie y fecha de compra). Unido a las horas de uso (idea 19)
      permite mostrar, por ejemplo, que la CNC comprada con el proyecto X se usó 340
      horas en ese proyecto.
- [ ] **60. Ejecución presupuestaria en vivo:** aprobado contra gastado por ítem,
      con alertas antes de pasarse.

### Rendición y auditoría
- [ ] **61. Rendiciones en el formato del financiador** (Excel o PDF) con todos los
      respaldos anexos.
- [ ] **65. Calendario de hitos:** informes técnicos y financieros, garantías y
      cierre de cada proyecto. Los hitos del Portafolio ya existen; falta llevarlos
      al calendario con avisos.
- [ ] **66. Expediente por proyecto exportable** (ZIP con índice y huellas) y acceso
      de solo lectura con fecha de término para un auditor.
- [ ] **67. Manual de procedimientos con historial de versiones:** cómo se mueve el
      dinero, escrito, y confirmado con firma por cada integrante. El software hace
      cumplir ese manual.

### Transparencia
- [ ] **70. Portal público de transparencia,** generado desde los datos reales (no
      a mano): proyectos, montos, bienes, uso, informes.
      - Es el módulo `transparencia`, que se activa y desactiva en Configuración >
        Módulos. A diferencia del resto, viene **apagado por defecto**, porque publica
        datos hacia afuera.
      - Apagado, la página pública responde «no encontrado» (no «prohibido»), para no
        revelar que existe.
      - Al activarlo, el administrador elige qué secciones se publican (proyectos,
        montos, bienes, horas de uso, informes) y ve una vista previa antes.
      - Cada activación, desactivación o cambio de secciones queda en la bitácora.
- [ ] **71. Solicitudes de información con seguimiento:** folio, plazo,
      responsable, respuesta y aviso cuando el plazo se acerca.

### Más adelante en la etapa
- [ ] **62. Horas dedicadas por persona y proyecto,** con boletas de honorarios y
      contratos: demuestra que quien recibió pago trabajó en ese proyecto.
- [ ] **63. Viajes y viáticos:** solicitud, aprobación, comprobantes e informe de
      comisión.
- [ ] **64. Caja chica e ingresos por servicios:** arqueos y destino del dinero que
      entra por el cotizador (idea 37).
- [ ] **68. Detector de anomalías:** facturas duplicadas, proveedores nuevos,
      compras divididas para no licitar (fraccionamiento).
- [ ] **69. Declaración de conflicto de interés** en cada compra y actas con las
      decisiones de gasto.

## Etapa 2 · Seguridad y monitoreo

- [ ] **20. Seguridad de la cortadora láser:** sensor de llama o humo que corta la
      energía, enciende la alarma y avisa por Telegram. El extractor se prende solo
      cuando la láser trabaja.
- [ ] **6. Alarmas críticas con escalamiento:** si un freezer, incubadora o sala se
      sale de rango, avisa por Telegram; si nadie responde en 10 minutos, avisa al
      siguiente de la lista.
- [ ] **7. Historial de temperaturas exportable** para auditorías y certificaciones.
- [ ] **19. Medidor de corriente en cada máquina** (ESP32 + sensor de pinza): sabe
      si la máquina trabaja de verdad, cuenta horas de uso y consumo eléctrico.
      Alimenta la idea 59.
- [ ] **22. Máquina fuera de servicio:** se escanea el QR, se reporta la falla con
      foto y se bloquean las reservas hasta la reparación.
- [ ] **17. Seguridad laboral:** inducciones, capacitaciones vigentes, fichas de
      seguridad y registro de incidentes.

## Etapa 3 · Acceso y uso de máquinas

- [ ] **21. Certificaciones por máquina:** tras la inducción la persona queda
      habilitada; eso desbloquea reservas y llave electrónica.
- [ ] **18. Llave electrónica por máquina:** ESP32 con lector de tarjeta y relé que
      habilita la máquina solo a quien tiene la certificación vigente y registra
      quién la usó y cuánto tiempo.
- [ ] **2. Reserva de equipos** con calendario sin choques de horario, exigencia de
      certificación y registro de horas.
- [ ] **5. Mantenimiento y calibración:** bitácora por equipo y recordatorios (por
      fecha o por horas de uso: boquilla tras X kg, lente láser tras X horas).
- [ ] **23. Una sola cola para todas las máquinas** (3D, láser, CNC, vinilo) con
      tiempo y costo estimados.
- [ ] **8 + 47. Pantalla de estado** (tablet o TV): máquinas en uso, tiempo
      restante, cola del día, temperaturas, alertas y ocupación.
- [ ] **48. Registro de entrada** con QR o tarjeta: cuántas personas vienen y de qué
      carrera.

## Etapa 4 · Materiales, costos y servicios

- [ ] **1 + 31 + 34. Ampliar el módulo Inventario** que ya existe (categorías,
      cantidad, costo, proveedor y filamento con gramos restantes): stock mínimo con
      aviso, vencimientos, fichas de seguridad de reactivos, componentes electrónicos
      y etiquetas QR por la impresora de LabNAS.
- [ ] **32. Registro de retazos:** foto y medidas de cada sobrante para reutilizarlo.
- [ ] **33. Préstamo de herramientas** con QR y aviso de devolución vencida.
- [ ] **35. Lista de materiales desde KiCad:** revisa el stock y arma el pedido de lo
      que falta.
- [ ] **36. Reciclaje:** kilos triturados y filamento recuperado, para métricas de
      sostenibilidad.
- [ ] **37. Cotizador automático:** gramos, superficie cortada, minutos de máquina y
      energía → precio.
- [ ] **38. Formulario público de pedidos** de fabricación con cotización y
      seguimiento.
- [ ] **39. Membresías y créditos de horas de máquina** para usuarios externos.
- [ ] **40. Talleres** con inscripción, cupos, asistencia y certificados en PDF.

## Etapa 5 · Fabricación

- [ ] **24. Laminado en el servidor:** se sube el STL y LabNAS genera el gcode con
      PrusaSlicer u OrcaSlicer usando los perfiles del laboratorio.
- [ ] **26. Visor en el navegador** para STL, STEP, DXF, SVG y vista previa de gcode.
- [ ] **30. Aviso al terminar** por Telegram con foto del resultado.
- [ ] **29. Base colaborativa de parámetros de corte** (potencia y velocidad por
      material y espesor, con fotos).
- [ ] **27. Acomodo automático de piezas** en la plancha de la láser.
- [ ] **28. Generadores paramétricos:** cajas con encastre, engranajes, bisagras
      flexibles → SVG.
- [ ] **25. Detección de fallas de impresión con IA:** la cámara reconoce la
      impresión arruinada y pausa la impresora.

## Etapa 6 · Docencia y conocimiento

- [ ] **41. Integración con Moodle:** un curso habilita máquinas, los estudiantes
      entregan su proyecto y la nota vuelve a Moodle.
- [ ] **42. Portafolio por estudiante** con documentación semanal (estilo Fab
      Academy). Distinto del módulo Portafolio actual, que registra proyectos,
      cursos y talleres del laboratorio.
- [ ] **43. Pauta de evaluación por proyecto** ligada a archivos y fotos.
- [ ] **16. Seguimiento de tesistas:** hitos, entregas y reuniones.
- [ ] **44. Biblioteca interna de proyectos:** archivos, fotos, parámetros usados y
      versiones derivadas.
- [ ] **45. Publicación con un clic** en GitHub o Printables con licencia abierta.
- [ ] **3. Cuaderno de laboratorio electrónico:** entradas con plantillas de
      protocolo, enlazadas a archivos y selladas con huella y fecha (prueba de
      autoría).
- [ ] **4. Registro de muestras:** ubicación (freezer, caja, posición), quién la tocó
      y cuándo, etiqueta QR.

## Etapa 7 · Datos, IA y red

- [ ] **12. Búsqueda por significado** en los documentos del NAS, citando los
      archivos.
- [ ] **13. Asistente por Telegram:** «¿dónde está el reactivo X?», «¿quién reservó
      el microscopio mañana?».
- [ ] **14. Transcripción y resumen de reuniones** con tareas propuestas para
      aprobar.
- [ ] **46. Asistente de fabricación:** parámetros de corte, revisión de STL
      imprimible, piezas en OpenSCAD a partir de una descripción.
- [ ] **9. Recepción automática desde instrumentos** vía carpeta compartida
      (requiere SMB).
- [ ] **10. Datos según buenas prácticas FAIR:** datos de contexto, plazos de
      conservación y publicación en Zenodo con DOI.
- [ ] **11. Cuotas por usuario y alertas de disco lleno.**
- [ ] **49. Reportes de impacto** para financiamiento: horas de máquina, usuarios,
      proyectos y talleres.
- [ ] **50. Plano interactivo del fablab** con el estado de cada máquina.
- [ ] **51. Red de laboratorios:** varios LabNAS comparten parámetros y biblioteca.

---

## Pendientes técnicos (de la auditoría de noviembre 2026)

- [ ] Algunos procesos en segundo plano usan la base de datos de forma bloqueante.
- [ ] Límites de tiempo para yt-dlp, lp, rsync y ffmpeg.
- [ ] Registrar en la auditoría la apertura de la terminal web.
- [ ] El visor de escritorio no verifica el certificado del NAS.
- [ ] Revisión automática de dependencias en el CI (cargo-audit, dependabot).
- [ ] Un tipo de error común en el backend.
- [ ] SMB/NFS (lo necesita la idea 9).
- [ ] Partir las páginas grandes del frontend (impresoras 3D, tareas, ajustes).

## Ya hecho (referencia)

Plan de noviembre 2026, en `master`: arreglos críticos de seguridad, correo con IA y
propuestas de agenda, cuentas Microsoft por OAuth2, puente PHP, respaldos restic
(compatibles con Backblaze B2 y Hetzner Storage Box, con `secret.key` incluida),
sesión en cookie HttpOnly con CSRF, passkeys, releases firmados con minisign,
snapshots btrfs/ZFS y servicio systemd endurecido. Detalle en `CHANGELOG.md`.

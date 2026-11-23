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

## Cómo es modular LabNAS hoy (revisión del código, noviembre 2026)

**Módulos.** Hay 13: `dashboard` (fijo), `files`, `network`, `printing`, `printers3d`,
`tasks`, `notes`, `email`, `inventory`, `portfolio`, `sensors`, `terminal`, `music`.
Se encienden, apagan y reordenan en **Configuración > Laboratorio**. Un módulo se
declara hoy en **cuatro lugares** que hay que mantener a mano:

| Dónde | Qué define |
|---|---|
| `backend/src/db.rs` → `DEFAULT_MODULES` | id y orden; siempre se crea **encendido** |
| `backend/src/middleware.rs` → `MODULE_ROUTE_PREFIXES` | rutas de la API; apagado responde 404 |
| `backend/src/main.rs` | tareas de fondo (correo, monitor 3D, sensores, música, red) |
| `frontend/src/components/Layout.tsx` → `moduleNav` | entrada del menú (ruta, nombre, ícono) |

El bus de eventos en vivo y WebDAV ya respetan el estado del módulo.

**Usuarios y permisos.** Cuatro roles globales (`Pendiente`, `Observador`, `Operador`,
`Admin`), tres permisos sueltos (`terminal`, `impresion`, `archivos_escritura`) y
permisos por carpeta en las raíces de almacenamiento. Cada ruta declara su nivel
mínimo y el middleware niega por defecto. Los módulos son **globales**: no se puede
encender un módulo solo para ciertos roles.

**Ajustes.** Tabla clave/valor (`settings`, con variantes cifradas para secretos) y
una sola página `SettingsPage.tsx` de ~1.850 líneas con 9 pestañas.

**Piezas reutilizables que ya existen:**

| Pieza | Dónde | Sirve de base para |
|---|---|---|
| Auditoría persistente (180 días, se purga) | `handlers/audit.rs`, `log_activity` | 72; la 57 necesita una cadena aparte que nunca se purgue |
| Reportes y solicitudes con estado y respuesta | `handlers/reports.rs` | 71, 22 |
| Proyectos con miembros y etiquetas, tareas con confirmación, calendario con recurrencia | módulo `tasks` | 2, 5, 16, 65 |
| Portafolio: proyectos, cursos, diplomas, talleres con requisitos e hitos | módulo `portfolio` | 40, 52, 65 |
| Inventario: costo, proveedor, ubicación, serie, fecha de compra, filamento | módulo `inventory` | 1/31/34, 36, 59 |
| Sensores ESP-NOW con token, lecturas y reglas de alerta (umbral y espera) | módulo `sensors` | 6, 7, 19, 20 |
| Impresoras 3D de varias marcas, cola, cámara, timelapse, detección de movimiento | módulo `printers3d` | 23, 25, 30 |
| Estadísticas de páginas y costo por usuario e impresora | `printing/stats.rs` | 37, 49 |
| Propuestas de la IA que una persona aprueba | `email/proposals.rs` | 14, 55 |
| Bot de Telegram con roles y comandos | `notifications/bot.rs` | 6, 13, 30 |
| Eventos en vivo con audiencias e intereses | `events.rs` | pantallas de estado (8/47) |
| Marca del laboratorio y servicios | Configuración > Laboratorio | 70 |

### Fallas encontradas en la revisión

- [x] **El bot de Telegram ignora los módulos apagados.** *(corregido)* Con el módulo Terminal
      apagado, `/cmd` sigue ejecutando comandos para quien tenga ese permiso. Lo mismo
      pasa con `/imprimir`, `/impresoras`, `/tareas`, `/propuestas` y `/musica`.
      Quien apaga un módulo espera que quede apagado del todo. **Prioridad alta**
      en el caso de la terminal.
- [x] **Las tareas de fondo solo miran el estado al arrancar.** *(corregido)* Encender el módulo
      Correo no inicia la revisión hasta reiniciar, y apagarlo no la detiene.
- [x] **No hay forma de que un módulo venga apagado por defecto** *(corregido: `DEFAULT_MODULES`
      lleva el estado inicial; lo usa `cameras` y lo usará `transparencia`)*.

---

## Etapa 0 · Base común

Lo que usan todas las demás etapas.

### Infraestructura de módulos
Con 7 módulos nuevos en camino, conviene ordenar esto primero:

- [ ] **Registro único de módulos en el backend:** id, nombre, ícono, rutas
      privadas y públicas, si viene encendido, de qué otros depende y sus tareas de
      fondo. El menú del frontend se arma desde `/api/modules` en vez de una lista fija.
      Agregar un módulo pasa de cuatro lugares a uno.
- [x] **Encendido por defecto configurable** (`cameras` ya viene apagado; `transparencia` también lo hará).
- [ ] **Dependencias entre módulos:** `transparencia` necesita `finanzas`, y
      `finanzas` usa `inventory` y `portfolio`. No se puede apagar uno del que otro
      depende sin avisar.
- [x] **Tareas de fondo que se encienden y apagan en vivo** junto con su módulo.
- [x] **Comandos de Telegram ligados a su módulo** (arregla la falla de arriba).
- [ ] **Ajustes propios de cada módulo:** cada módulo trae su sección de
      configuración, que solo aparece si está encendido. De paso se parte
      `SettingsPage.tsx`.
- [ ] **Tarjetas del dashboard por módulo**, que aparecen solo si está encendido.

### Personas y acceso
- [ ] **Permisos por módulo y por rol:** cada rol tiene, en cada módulo, «sin
      acceso», «lectura» o «escritura». Reemplaza los tres permisos sueltos de hoy
      (`terminal`, `impresion`, `archivos_escritura`) por algo uniforme, y permite que
      `finanzas` lo vean solo el responsable y el administrativo.
- [ ] **Roles nuevos sobre los cuatro actuales:** investigador responsable,
      administrativo, docente, estudiante, usuario externo, auditor (solo lectura y con
      fecha de término).
- [ ] **Niveles de acceso** privado / con autorización / público, aplicables a
      cualquier registro o archivo.
- [x] **Catálogo único de máquinas y bienes con QR:** cada equipo tiene una ficha
      a la que llevan las ideas de reservas, mantenimiento, inventario y uso.
      *(hecho: módulo Máquinas, ver `docs/MAQUINAS.md`; los bienes de inventario aún no tienen QR)*
### Registro y catálogo
- [x] **57. Bitácora que no se puede alterar.** Cada movimiento queda encadenado al
      anterior con su huella digital (hash). Nada se borra ni se edita: un error se
      corrige con un registro nuevo que lo anula. Si alguien adulterara un registro,
      la cadena lo delataría. *(hecho: `ledger.rs`, notas con huella de archivos,
      anulaciones, exportación verificable con Python y ancla diaria por Telegram.
      Detalle en `docs/BITACORA.md`)*
- [ ] **72. Registro de cada acceso:** quién vio o descargó qué y cuándo. Amplía
      la auditoría que ya existe (`audit.rs`), que hoy registra acciones pero no
      lecturas.

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
      al calendario. *(Los avisos ya están: módulo Plazos, idea 73.)*
- [x] **73. Contadores de tiempo** (cuenta regresiva) para el cierre de cada proyecto
      y otros plazos: informes técnicos y financieros, rendiciones, garantías,
      convocatorias a fondos, entregas de tesis, vencimiento de licencias o
      certificaciones. Tarjetas en el dashboard que cambian de color al acercarse la
      fecha (por ejemplo verde > 30 días, amarillo < 30, rojo < 7) y avisos por
      Telegram en hitos fijos (30, 7 y 1 día). Se alimenta de los hitos de la idea 65
      y del Portafolio, y también permite crear contadores sueltos.
      *Hecho: módulo Plazos (`docs/PLAZOS.md`), con aviso también el mismo día.*
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
      responsable, respuesta y aviso cuando el plazo se acerca. Parte de los reportes
      que ya existen (tipo, estado y respuesta del administrador), abiertos al público
      y con plazo.

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
      cuando la láser trabaja. Los sensores hoy solo envían lecturas: falta que LabNAS
      pueda **mandar órdenes** a un relé (también lo necesita la idea 18).
- [ ] **6. Alarmas críticas con escalamiento:** si un freezer, incubadora o sala se
      sale de rango, avisa por Telegram; si nadie responde en 10 minutos, avisa al
      siguiente de la lista. Los sensores ya tienen reglas de alerta con umbral; falta
      el escalamiento y la confirmación «lo vi».
- [ ] **7. Historial de temperaturas exportable** para auditorías y certificaciones.
- [ ] **19. Medidor de corriente en cada máquina** (ESP32 + sensor de pinza): sabe
      si la máquina trabaja de verdad, cuenta horas de uso y consumo eléctrico.
      Alimenta la idea 59.
- [x] **22. Máquina fuera de servicio:** se escanea el QR, se reporta la falla con
      foto y se bloquean las reservas hasta la reparación. Puede reutilizar los
      reportes que ya existen. *(hecho en el módulo Máquinas, con foto y aviso por Telegram)*
- [ ] **17. Seguridad laboral:** inducciones, capacitaciones vigentes, fichas de
      seguridad y registro de incidentes.

## Etapa 3 · Acceso y uso de máquinas

- [x] **21. Certificaciones por máquina:** tras la inducción la persona queda
      habilitada; eso desbloquea reservas y llave electrónica. *Hecho en el módulo
      Máquinas: se pide desde la ficha, la da un operador con vencimiento opcional y los
      vencimientos salen en Plazos. Falta la llave electrónica (idea 18).*
- [ ] **18. Llave electrónica por máquina:** ESP32 con lector de tarjeta y relé que
      habilita la máquina solo a quien tiene la certificación vigente y registra
      quién la usó y cuánto tiempo.
- [ ] **2. Reserva de equipos** con calendario sin choques de horario, exigencia de
      certificación y registro de horas. *(en parte: el módulo Máquinas ya reserva sin
      choques, con aprobación opcional, recordatorio y exigencia de inducción; falta
      registrar las horas reales de uso —idea 19—)*
- [ ] **5. Mantenimiento y calibración:** bitácora por equipo y recordatorios (por
      fecha o por horas de uso: boquilla tras X kg, lente láser tras X horas).
- [ ] **23. Una sola cola para todas las máquinas** (3D, láser, CNC, vinilo) con
      tiempo y costo estimados. Generaliza la cola que ya tiene el módulo de
      impresoras 3D.
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
      energía → precio. La impresión de papel ya calcula costo por usuario; se extiende
      al resto de las máquinas.
- [ ] **38. Formulario público de pedidos** de fabricación con cotización y
      seguimiento.
- [ ] **39. Membresías y créditos de horas de máquina** para usuarios externos.
- [ ] **40. Talleres** con inscripción, cupos, asistencia y certificados en PDF. El
      Portafolio ya registra talleres; falta la inscripción.

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
- [ ] **13. Asistente por Telegram** (sobre el bot que ya existe): «¿dónde está el reactivo X?», «¿quién reservó
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

- [x] Algunos procesos en segundo plano usan la base de datos de forma bloqueante. *(corregido: `db::bg`)*
- [x] Límites de tiempo para yt-dlp, lp, rsync y ffmpeg.
- [x] Registrar en la auditoría la apertura de la terminal web.
- [x] El visor de escritorio no verifica el certificado del NAS. *(corregido: huella fijada al primer uso)*
- [x] Revisión automática de dependencias en el CI (cargo-audit, dependabot).
- [ ] Un tipo de error común en el backend.
- [ ] SMB/NFS (lo necesita la idea 9).
- [ ] Partir las páginas grandes del frontend (impresoras 3D, tareas, ajustes).

## Ya hecho (referencia)

Plan de noviembre 2026, en `master`: arreglos críticos de seguridad, correo con IA y
propuestas de agenda, cuentas Microsoft por OAuth2, puente PHP, respaldos restic
(compatibles con Backblaze B2 y Hetzner Storage Box, con `secret.key` incluida),
sesión en cookie HttpOnly con CSRF, passkeys, releases firmados con minisign,
snapshots btrfs/ZFS y servicio systemd endurecido. Detalle en `CHANGELOG.md`.

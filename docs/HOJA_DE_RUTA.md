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
   (registro de módulos en `backend/src/modules.rs`), incluidas las públicas. Módulos propuestos: `finanzas`
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

**Módulos.** Hay 16: `dashboard` (fijo), `files`, `network`, `printing`, `printers3d`,
`tasks`, `notes`, `email`, `inventory`, `portfolio`, `sensors`, `terminal`, `music`,
`cameras`, `machines` y `deadlines`. Se encienden, apagan y reordenan en
**Configuración > Laboratorio**. Desde noviembre de 2026 un módulo se declara en un
**registro único** (`backend/src/modules.rs`): id, nombre, título, ícono, ruta en la
interfaz, rutas de la API (privadas y públicas), si viene encendido, si es solo de la
LAN y qué otros módulos usa. El menú, el título de cada página, la lista de módulos en
Configuración y los nombres en roles y Telegram salen de ahí vía `/api/modules`.
Fuera del registro quedan, a propósito, solo las cosas que son código:

| Dónde | Qué define |
|---|---|
| `frontend/src/modules.tsx` | qué componente dibuja la página del módulo |
| `backend/src/roles.rs` → `template_level` | nivel del módulo en cada plantilla de rol |
| `backend/src/module_tasks.rs` | tareas de fondo, si tiene |
| `frontend/src/lib/moduleIcons.ts` | el ícono, si es uno que aún no se usa |

El bus de eventos en vivo y WebDAV ya respetan el estado del módulo.

**Usuarios y permisos.** Roles editables (tabla `roles`) con un nivel por módulo
(sin acceso / ver / usar / gestionar), excepciones por persona y fecha de término de la
cuenta; además, permisos por carpeta en las raíces de almacenamiento. Cada ruta declara
el nivel que exige en su módulo y el middleware niega por defecto. Detalle en
`docs/ROLES.md`.

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

- [x] **Registro único de módulos en el backend:** id, nombre, ícono, rutas
      privadas y públicas, si viene encendido, de qué otros depende y sus tareas de
      fondo. El menú del frontend se arma desde `/api/modules` en vez de una lista fija.
      Agregar un módulo pasa de cuatro lugares a uno. *(hecho: `backend/src/modules.rs`;
      reemplaza `DEFAULT_MODULES`, `MODULE_ROUTE_PREFIXES`, `MODULE_USES`, las rutas
      públicas por módulo y cinco listas fijas del frontend. Las tareas de fondo siguen en
      `module_tasks.rs` porque son código)*
- [x] **Encendido por defecto configurable** (`cameras` ya viene apagado; `transparencia` también lo hará).
- [x] **Dependencias entre módulos:** `transparencia` necesita `finanzas`, y
      `finanzas` usa `inventory` y `portfolio`. No se puede apagar uno del que otro
      depende sin avisar. *(hecho: `db::MODULE_USES`; hoy Correo usa Tareas, Plazos usa
      Portafolio y Cámaras usa Impresoras 3D. Apagar uno que otro usa pide confirmación y
      la lista de módulos muestra qué usa cada uno)*
- [x] **Tareas de fondo que se encienden y apagan en vivo** junto con su módulo.
- [x] **Comandos de Telegram ligados a su módulo** (arregla la falla de arriba).
- [x] **Ajustes propios de cada módulo:** cada módulo trae su sección de
      configuración, que solo aparece si está encendido. De paso se parte
      `SettingsPage.tsx`. *(hecho: pestaña **Configuración > Módulos**, armada desde
      `MODULE_SETTINGS` en `frontend/src/modules.tsx`; hoy Correo (app de Microsoft),
      Sensores (puerto del receptor), Música (Last.fm y mpv, antes pestaña Multimedia) y
      Cámaras (carpeta y duración). Los botones de ajustes de cada página llevan ahí con
      `?tab=modules&seccion=modulo-<id>`. `SettingsPage.tsx` ya estaba partido en pestañas)*
- [x] **Tarjetas del dashboard por módulo**, que aparecen solo si está encendido.
      *(hecho: Plazos, Impresoras 3D, Red, Tareas, Máquinas y Sensores; también se ocultan
      si el rol no ve el módulo)*

### Personas y acceso
- [x] **Permisos por módulo y por rol:** cada rol tiene, en cada módulo, «sin
      acceso», «ver», «usar» o «gestionar». Reemplaza los tres permisos sueltos de antes
      (`terminal`, `impresion`, `archivos_escritura`) por algo uniforme, y permite que
      `finanzas` lo vean solo el responsable y el administrativo. *(hecho: roles editables
      con excepciones por persona, ver `docs/ROLES.md`)*
- [x] **Roles nuevos sobre los cuatro actuales:** investigador responsable,
      administrativo, docente, estudiante, usuario externo, auditor (solo lectura y con
      fecha de término). *(hecho, más encargado técnico y ayudante; la fecha de término
      vale para cualquier cuenta)*
- [x] **Niveles de acceso** privado / con autorización / público, aplicables a
      cualquier registro o archivo. *(hecho: `docs/ACCESO.md`; proyectos, bienes, notas,
      plazos y archivos, con autorizaciones por persona con fecha de término, enlace
      público, «Compartido conmigo» y todo en la auditoría. Es la base de la idea 66
      —acceso de un auditor— y del portal de transparencia, idea 70)*
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
- [x] **72. Registro de cada acceso:** quién vio o descargó qué y cuándo. Amplía
      la auditoría que ya existe (`audit.rs`), que hoy registra acciones pero no
      lecturas. *(hecho: `state.log_access`, acción «Acceso» en la auditoría con filtro
      propio; descargas del explorador, vistas previas, enlaces públicos, WebDAV, cámaras y
      grabaciones. El mismo acceso se anota una vez cada 10 minutos)*

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
- [x] **6. Alarmas críticas con escalamiento:** si un freezer, incubadora o sala se
      sale de rango, avisa por Telegram; si nadie responde en 10 minutos, avisa al
      siguiente de la lista. Los sensores ya tienen reglas de alerta con umbral; falta
      el escalamiento y la confirmación «lo vi». *(hecho: `docs/ALARMAS.md`; alerta
      crítica con lista en orden, `/visto` por Telegram o «Lo vi» en la web, aviso al
      cerrar e historial de incidentes)*
- [x] **7. Historial de temperaturas exportable** para auditorías y certificaciones.
      *(hecho: `docs/SENSORES.md`; CSV por rango de fechas, resumen min/máx/promedio,
      tiempo fuera de rango por alerta e informe imprimible; cada exportación queda en
      el registro de accesos)*
- [ ] **19. Medidor de corriente en cada máquina** (ESP32 + sensor de pinza): sabe
      si la máquina trabaja de verdad, cuenta horas de uso y consumo eléctrico.
      Alimenta la idea 59.
- [x] **22. Máquina fuera de servicio:** se escanea el QR, se reporta la falla con
      foto y se bloquean las reservas hasta la reparación. Puede reutilizar los
      reportes que ya existen. *(hecho en el módulo Máquinas, con foto y aviso por Telegram)*
- [x] **17. Seguridad laboral:** inducciones, capacitaciones vigentes, fichas de
      seguridad y registro de incidentes. *(hecho en el módulo Seguridad: capacitaciones
      generales con vigencia y vencimientos en Plazos, fichas con archivo adjunto y vínculo a
      máquina o inventario, e incidentes con aviso por Telegram si son graves; ver SEGURIDAD.md)*

## Etapa 3 · Acceso y uso de máquinas

- [x] **21. Certificaciones por máquina:** tras la inducción la persona queda
      habilitada; eso desbloquea reservas y llave electrónica. *Hecho en el módulo
      Máquinas: se pide desde la ficha, la da un operador con vencimiento opcional y los
      vencimientos salen en Plazos. Falta la llave electrónica (idea 18).*
- [ ] **18. Llave electrónica por máquina:** ESP32 con lector de tarjeta y relé que
      habilita la máquina solo a quien tiene la certificación vigente y registra
      quién la usó y cuánto tiempo.
- [x] **2. Reserva de equipos** con calendario sin choques de horario, exigencia de
      certificación y registro de horas. *(hecho en el módulo Máquinas: reserva sin
      choques, con aprobación opcional, recordatorio y exigencia de inducción; y registro
      de uso real con «Empezar a usar / Terminar» desde la ficha, horas por máquina,
      persona y proyecto. Con medidor de corriente —idea 19— se medirá solo)*
- [x] **5. Mantenimiento y calibración:** bitácora por equipo y recordatorios (por
      fecha o por horas de uso: boquilla tras X kg, lente láser tras X horas). *(hecho en
      el módulo Máquinas: plan por días y/o horas de uso, las de fecha en Plazos y las de
      horas con aviso por Telegram, tareas que bloquean reservas al vencer, bitácora con
      foto y horómetro. Las horas salen de las reservas hasta que exista el medidor de la
      idea 19; los kilos de filamento quedan pendientes)*
- [ ] **23. Una sola cola para todas las máquinas** (3D, láser, CNC, vinilo) con
      tiempo y costo estimados. Generaliza la cola que ya tiene el módulo de
      impresoras 3D.
- [x] **8 + 47. Pantalla de estado** (tablet o TV): máquinas en uso, tiempo
      restante, cola del día, temperaturas, alertas y ocupación. *(hecho:
      `docs/PANTALLA.md`; enlace secreto de solo lectura por pantalla, con secciones a
      elegir. La ocupación de personas llega con el registro de entrada, idea 48)*
- [x] **48. Registro de entrada** con QR o tarjeta: cuántas personas vienen y de qué
      carrera. *(hecho con QR en el módulo Seguridad, `docs/SEGURIDAD.md`: entrada y
      salida, visitas, quién está ahora y estadísticas por día y carrera; la tarjeta queda
      para la llave electrónica, idea 18)*

## Etapa 4 · Materiales, costos y servicios

- [ ] **1 + 31 + 34. Ampliar el módulo Inventario** que ya existe (categorías,
      cantidad, costo, proveedor y filamento con gramos restantes): stock mínimo con
      aviso, vencimientos, fichas de seguridad de reactivos, componentes electrónicos
      y etiquetas QR por la impresora de LabNAS. *(hecho en parte, `docs/INVENTARIO.md`:
      stock mínimo con aviso, vencimientos en Plazos y etiqueta QR por ítem; faltan fichas
      de seguridad y la lista de componentes electrónicos)*
- [x] **32. Registro de retazos:** foto y medidas de cada sobrante para reutilizarlo.
      *(hecho en Inventario: búsqueda por material, espesor y tamaño de la pieza)*
- [x] **33. Préstamo de herramientas** con QR y aviso de devolución vencida. *(hecho:
      ítems prestables, préstamo con fecha de devolución y aviso diario si se atrasa)*
- [ ] **35. Lista de materiales desde KiCad:** revisa el stock y arma el pedido de lo
      que falta.
- [x] **36. Reciclaje:** kilos triturados y filamento recuperado, para métricas de
      sostenibilidad. *(hecho en Inventario: movimientos por material, totales del año,
      porcentaje reaprovechado y gráfico por mes)*
- [ ] **37. Cotizador automático:** gramos, superficie cortada, minutos de máquina y
      energía → precio. La impresión de papel ya calcula costo por usuario; se extiende
      al resto de las máquinas.
- [ ] **38. Formulario público de pedidos** de fabricación con cotización y
      seguimiento.
- [ ] **39. Membresías y créditos de horas de máquina** para usuarios externos.
- [x] **40. Talleres** con inscripción, cupos, asistencia y certificados en PDF. El
      Portafolio ya registra talleres; falta la inscripción. *(hecho: inscripción con
      sesión o por enlace público, cupos con lista de espera, asistencia por sesión y
      certificados con código verificable; ver [TALLERES.md](TALLERES.md))*

## Etapa 5 · Fabricación

- [ ] **24. Laminado en el servidor:** se sube el STL y LabNAS genera el gcode con
      PrusaSlicer u OrcaSlicer usando los perfiles del laboratorio.
- [ ] **26. Visor en el navegador** para STL, STEP, DXF, SVG y vista previa de gcode.
- [x] **30. Aviso al terminar** por Telegram con foto del resultado. *(foto al personal y a quien encargó la pieza; texto al resto)*
- [x] **29. Base colaborativa de parámetros de corte** (potencia y velocidad por
      material y espesor, con fotos). *(hecho en Máquinas: por máquina, con foto del
      resultado, «me funcionó» y búsqueda por material)*
- [ ] **27. Acomodo automático de piezas** en la plancha de la láser.
- [ ] **28. Generadores paramétricos:** cajas con encastre, engranajes, bisagras
      flexibles → SVG.
- [x] **25. Detección de fallas de impresión con IA:** la cámara reconoce la
      impresión arruinada y pausa la impresora. *(con Obico aparte, `setup-obico.sh`; pendiente: modelo propio)*

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
- [x] Un tipo de error común en el backend. *(`error.rs`: `ApiError`, `internal`, `db_err`)*
- [ ] SMB/NFS (lo necesita la idea 9).
- [x] Partir las páginas grandes del frontend (impresoras 3D, tareas, ajustes).

## Ya hecho (referencia)

Plan de noviembre 2026, en `master`: arreglos críticos de seguridad, correo con IA y
propuestas de agenda, cuentas Microsoft por OAuth2, puente PHP, respaldos restic
(compatibles con Backblaze B2 y Hetzner Storage Box, con `secret.key` incluida),
sesión en cookie HttpOnly con CSRF, passkeys, releases firmados con minisign,
snapshots btrfs/ZFS y servicio systemd endurecido. Detalle en `CHANGELOG.md`.

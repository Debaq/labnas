# Puente PHP: LabNAS desde internet sin abrir puertos

> Estado: **etapas B1–B4 implementadas** (`bridge/`, `backend/src/bridge.rs`, `handlers/uploads.rs`, long-poll en `events.rs`). Versión 2, 2026-11-18 (ajustado al hosting real: PHP 7+, Apache, carpetas fuera de la web, un solo LabNAS).

## 1. Objetivo

Usar LabNAS desde internet a través de una web PHP en un **hosting compartido** (sin VPS, sin procesos permanentes, sin abrir puertos en el router del laboratorio).

- **Configurable:** el conector PHP no trae nada fijo. Cada instalación se enlaza con **un** LabNAS mediante un código (cada persona instala su propio conector y pone su dirección).
- **Entregado por LabNAS:** LabNAS ofrece para descarga el paquete del conector de su misma versión y lo actualiza solo por un canal firmado.
- **Alcance acordado:** toda la web de LabNAS **menos la terminal y las rutas de admin**. Esas se usan en la LAN o por Tailscale.

## 2. Por qué la conexión va "al revés"

LabNAS está detrás del NAT del laboratorio, así que el hosting no puede conectarse a él. Por eso es **LabNAS quien sale hacia el hosting** y le pregunta si hay pedidos. Este patrón se llama *relay con long-polling*.

```
                     internet                         │ red del laboratorio
                                                      │
Navegador ──HTTPS──▶ labnas.tudominio.cl (PHP)        │
   ▲                  │  1. guarda el pedido en cola  │
   │                  │  4. devuelve la respuesta     │
   │                  ▼                               │
   │            cola en disco ◀──── 2. "¿hay pedidos?" (long-poll firmado) ──── LabNAS
   └──────────────────┘     ◀──── 3. respuesta firmada ─────────────────────────┘
```

No se abre ningún puerto en el laboratorio. LabNAS solo hace conexiones HTTPS salientes hacia el hosting.

## 3. Piezas

### 3.1 Conector PHP (en el hosting)

| Archivo | Rol |
|---|---|
| `index.php` | **Cargador mínimo.** Nunca se autoactualiza. Incluye `app/<versión>/` y, si esa versión falla al cargar, vuelve a la anterior |
| `app/<versión>/` | Código del puente: front controller, cola, protocolo y autoactualización |
| `app/<versión>/web/` | **Frontend de LabNAS compilado, de la misma versión.** El puente lo sirve directo, así que el HTML/JS/CSS no pasa por el relay |
| `instalar.php` | Asistente de primera vez. Se borra o desactiva al terminar |
| `~/labnas-data/` | Configuración, clave y cola, **fuera de `public_html`** (el hosting lo permite). La ruta se elige en el instalador, que verifica que exista, se pueda escribir y no sea accesible desde la web |

**Servidor: Apache** con `.htaccess` (`mod_rewrite` envía todo a `index.php`). No necesita MySQL: la cola usa archivos con `flock`.

**Compatibilidad: PHP 7.0 a 8.x, con el mismo código.** Reglas para escribir el conector:
- Nada de lo que agregó PHP 7.1 en adelante: tipos nulables (`?string`), `void`, propiedades con tipo, funciones flecha (`fn`), `??=`, `match`, argumentos con nombre, `str_contains`/`str_starts_with`, `#[...]` ni `readonly`. En su lugar, `isset`/`??`, `strpos(...) === 0` y `switch`.
- Solo funciones del núcleo que existen desde 7.0: `hash_hmac`, `hash_equals`, `random_bytes`, `json_encode`/`json_decode`, `flock`, `rename`, `bin2hex`.
- Sin dependencias de Composer ni extensiones opcionales: ni `zip`, ni `curl` (PHP no hace llamadas salientes; siempre es LabNAS quien llama), ni `sodium` (llegó en 7.2).
- El CI prueba el conector con **PHP 7.0, 7.4, 8.1 y 8.3** (`shivammathur/setup-php`), y además pasa `php -l` en cada versión.
- Nota: PHP 7.x ya no recibe parches de seguridad. Funciona igual, pero si el panel deja elegir, conviene la versión 8 más nueva.

**Un solo LabNAS por conector**, en un subdominio propio (por ejemplo `labnas.tudominio.cl`) para que el frontend corra en `/` sin cambios. Si el hosting no deja crear subdominios, hay una alternativa en subcarpeta (`tudominio.cl/labnas/`): el puente inyecta `<base href>` y el frontend antepone esa ruta a `/api` (es un cambio chico en `client.ts` y en el router).

### 3.2 Cliente del puente (dentro de LabNAS)

- Una tarea en segundo plano con **2 a 4 "pollers"** concurrentes, para que una descarga lenta no bloquee al resto.
- Cada pedido se ejecuta **dentro del mismo proceso** contra el router de axum (`tower::ServiceExt::oneshot`), sin pasar por HTTP local. Se le agrega la marca `Origin::Bridge`.
- Configuración (admin): URL del puente, activado/desactivado, número de pollers y tope de tamaño. El estado (conectado, latencia, último error) se ve en Configuración.

## 4. Enlace (configurable por cada uno)

1. Subes el zip del conector, que se descarga desde LabNAS (sección 7), al subdominio.
2. Abres `https://labnas.tudominio.cl/instalar.php`. El instalador:
   - verifica la versión de PHP, `mod_rewrite` y la carpeta de datos fuera de `public_html` (que se pueda escribir y no sea visible desde la web);
   - **mide los límites del hosting**: `upload_max_filesize`, `post_max_size`, `max_execution_time`, `memory_limit` y si `flush()` llega de verdad al navegador. Los muestra y LabNAS ajusta a ellos el tamaño de las partes y los tiempos de espera, así que no hace falta saberlos de antemano;
   - genera un **código de instalación** de un solo uso y válido por 30 minutos. Lo muestra una sola vez y guarda solo su hash.
3. En LabNAS, en Configuración → Acceso remoto → Puente PHP, pegas la URL del puente y el código de instalación.
4. LabNAS genera `node_id` (16 bytes) y la clave `K` (32 bytes, guardada cifrada como los demás secretos) y llama a `register` por HTTPS con el código. El puente guarda `K` para ese nodo y quema el código.
5. A partir de ahí, todo el canal LabNAS↔puente va firmado con `K`. El instalador queda desactivado.

Para cambiar de hosting basta con repetir el enlace con otra URL. Para revocar el puente basta con borrarlo en LabNAS (la clave deja de valer).

## 5. Protocolo LabNAS ↔ puente

Todas las llamadas de LabNAS son `POST https://puente/_labnas/<op>` con estas cabeceras:

```
X-Node: <node_id>
X-Ts: <unix segundos>
X-Nonce: <16 bytes aleatorios>
X-Sig: HMAC-SHA256(K, op \n ts \n nonce \n sha256(cuerpo))
```

El puente rechaza cualquier llamada con más de 60 s de desfase o con un nonce repetido; guarda los nonces 5 minutos.

| op | Qué hace |
|---|---|
| `register` | Enlace inicial (autorizado con el código de instalación, no con `K`) |
| `poll` | Espera hasta 20 s a que haya pedidos y devuelve un lote (máx. 8) con método, ruta, query, cabeceras permitidas, IP del cliente y cuerpo si es chico. Si no hay nada, responde vacío y LabNAS repite |
| `body` | Descarga en partes el cuerpo de un pedido grande (subidas) |
| `respond` | Estado, cabeceras y cuerpo de una respuesta; si es grande, en partes (`seq`, `final`) |
| `status` | Versión del conector, espacio libre, límites de PHP (`upload_max_filesize`, `post_max_size`, `max_execution_time`) |
| `update` | Autoactualización (sección 7) |

### Del lado del navegador

- **`/api/*`**: el puente escribe el pedido en `cola/<nodo>/<id>.req` y espera la respuesta revisando cada ~100 ms, como máximo 25 s; si no llega, responde `504 LabNAS no responde`.
  - Respuestas en partes: se envían al navegador a medida que llegan (`flush`, `X-Accel-Buffering: no`).
  - Se respetan `Range` y `Content-Disposition`, así que el video y las descargas funcionan.
- **Todo lo demás**: archivos estáticos del frontend desde `web/`, con fallback a `index.html`.
- **Cabeceras:** solo pasan las de una lista permitida (`Authorization`, `Content-Type`, `Range`, `If-*`, `Accept*`). La IP real va en `X-Forwarded-For`, que LabNAS solo acepta si el pedido vino por el relay.

### Latencia y límites esperados

- **Latencia:** con un poll ya en espera, un pedido simple tarda entre 0,3 y 1 s en total.
- **Tamaño de subidas:** cada parte respeta `post_max_size` del hosting. El total lo limita el ajuste de LabNAS y el disco del hosting.
- **Procesos de PHP:** cada poll y cada navegador esperando ocupa un proceso PHP, y los hostings limitan los concurrentes ("entry processes", típicamente 10–30). Por eso LabNAS arranca con **2 pollers** y el estado del puente muestra cuántos procesos se usan. Si el hosting empieza a responder 503/508, LabNAS baja a 1 poller y lo avisa.
- **Tiempo de espera:** si `max_execution_time` es bajo (por ejemplo 30 s), el long-poll se acorta solo para quedar por debajo (20 s, o el límite menos 5 s).

## 6. Seguridad

| Riesgo | Medida |
|---|---|
| Terminal y admin expuestos a internet | Si el pedido trae `Origin::Bridge`, el middleware deniega la terminal, `/api/live` (WebSocket), WebDAV y **todo lo que exige rol Admin**. La respuesta es 403 con "No disponible por el puente". Hay tests de esto |
| Alguien llama al puente haciéndose pasar por LabNAS | HMAC con `K`, ventana de 60 s y nonces de un solo uso |
| Alguien instala su propio LabNAS en tu puente | El código de instalación es de un solo uso, vence a los 30 min y solo se guarda su hash |
| Fuerza bruta de contraseñas desde internet | Se mantiene el bloqueo por usuario y se agrega un límite global para logins por puente. **Registro de cuentas cerrado por el puente** por defecto |
| Robo de sesión | Las sesiones creadas por el puente duran menos (8 h). Hay una opción para exigir 2FA en logins por puente y un aviso (toast + Telegram) de "nuevo inicio de sesión por el puente" |
| **El hosting ve el tráfico en claro** (el HTTPS termina ahí) | No se puede evitar con un navegador normal: confías en el hosting igual que en cualquier web que administres. Por eso admin y terminal quedan fuera. Recomendación: usar 2FA |
| Hosting comprometido y código PHP alterado | El atacante podría leer sesiones mientras dure el compromiso, pero no puede ejecutar comandos ni tocar admin porque LabNAS lo bloquea en su propio lado. Revocar el puente en LabNAS lo corta en el acto |
| Datos en el hosting | Las cargas y respuestas en cola se borran al entregarse y, en todo caso, a los 2 minutos. No quedan archivos de LabNAS en el hosting |

## 7. Paquete del conector y actualización

- **Build:** el CI de release genera `labnas-bridge-<versión>.zip` con:
  - el PHP;
  - el `dist/` del frontend;
  - `manifest.json`, con el sha256 de cada archivo y la versión.
- El zip va dentro del tarball de LabNAS, con su `.sha256` como el resto de los assets.
- **Descarga:** en Configuración → Acceso remoto → "Descargar conector PHP" (`GET /api/bridge/package`, solo admin y solo en la LAN).
- **Autoactualización:** cuando LabNAS se actualiza (o al arrancar) compara su versión con la del puente, que viene en `status`. Si son distintas:
  1. Envía con `update` el manifiesto y luego los archivos en partes, todo firmado con `K`.
  2. El puente escribe en `app/<versión-nueva>.staging/` y verifica cada sha256 contra el manifiesto.
  3. Renombra la carpeta (operación atómica) y actualiza el puntero que lee `index.php`. Conserva la versión anterior para volver atrás.
  4. Si falla cualquier paso, sigue funcionando con la versión anterior y LabNAS muestra el error.

## 8. Qué funciona y qué no por el puente

| Funciona | No funciona (usar LAN o Tailscale) |
|---|---|
| Archivos (explorar, subir y bajar en partes, previsualizar video con `Range`), papelera, links compartidos | Terminal web (WebSocket) |
| Correo y propuestas de la IA, tareas, calendario, notas, inventario, portafolio | Todo lo de admin: usuarios, módulos, respaldos, actualizar, apagar, raíces, TLS |
| Impresoras 3D (estado, cola, cámara como imagen), impresión, sensores, red (ver) | — |
| Tiempo real (avisos, propuestas, estado) por long-poll, con ~1 s de demora | — |
| Login con 2FA, cambio de contraseña propia | WebDAV |

## 9. Cambios en LabNAS que esto requiere

1. `bridge.rs`: cliente con pollers, despacho en proceso por `oneshot`, firma y estado. Ajustes en `settings` y la clave en secretos.
2. Middleware: `Origin::Bridge` y su lista de denegados, el límite global de logins y `X-Forwarded-For` solo desde el relay.
3. Sesiones: columna `origin` y TTL más corto para las del puente; aviso de login nuevo.
4. Eventos en vivo sin WebSocket: `GET /api/live/poll?ticket&since` (long-poll de 20 s sobre el mismo bus). El `EventsProvider` lo usa si el WebSocket falla.
5. Frontend: sin cambios de ruta base, gracias al subdominio en la raíz. Hay que ocultar la terminal y las secciones de admin cuando `/api/health` diga `via: "bridge"`.
6. CI: armar el zip del conector y un **test de punta a punta con `php -S`** (el runner de Ubuntu trae PHP). Ese test levanta el puente y LabNAS y recorre login, archivos, descargas grandes, denegaciones y actualización.

## 10. Plan por etapas

| Etapa | Contenido | Resultado |
|---|---|---|
| B1 | Enlace, protocolo firmado, cola, pollers, pedidos y respuestas chicos (≤ 8 MB), política de denegación, frontend servido por el puente | Ya se usa la web por el puente |
| B2 | Cuerpos en partes: subidas y descargas grandes, `Range` | Archivos y videos grandes |
| B3 | Zip en el release, descarga desde LabNAS, autoactualización con vuelta atrás | Mantenimiento sin tocar el hosting |
| B4 | Long-poll de eventos, avisos de login por el puente, panel de estado | Tiempo real y visibilidad |

## 11. Decisiones tomadas

| Tema | Decisión |
|---|---|
| PHP | 7.0 a 8.x con el mismo código (sección 3.1); CI con 7.0, 7.4, 8.1 y 8.3 |
| Servidor | Apache con `.htaccess` |
| Datos del puente | Fuera de `public_html` |
| Límites del hosting | Desconocidos: el instalador los mide y LabNAS se adapta |
| LabNAS por conector | Uno |

**Por confirmar:** si se puede crear un subdominio con HTTPS. Si no, se usa la variante en subcarpeta de la sección 3.1.

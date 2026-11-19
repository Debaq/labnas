# Bitácora inalterable

Idea 57 de la [hoja de ruta](HOJA_DE_RUTA.md). Es la base de la trazabilidad
financiera: aquí escribirán los módulos de finanzas y transparencia, y mientras tanto
sirve para dejar constancia de decisiones, aprobaciones y documentos.

Está en **Configuración > Admin > Bitácora inalterable** y por ahora solo la usa el
administrador. Cuando existan los permisos por módulo, el rol auditor podrá leerla. No
está disponible por el puente de internet.

## Cómo funciona

- Cada registro guarda quién, qué, cuándo y la **huella** (SHA-256) del registro
  anterior. Si alguien cambiara un registro antiguo, su huella ya no cuadraría con la
  que guardó el siguiente, y la verificación lo indicaría.
- **Nada se edita ni se borra.** La base de datos lo impide con triggers. Un error se
  corrige con un registro de tipo `anulacion` que indica el motivo; el original queda
  visible y tachado. Un registro se anula una sola vez, y una anulación no se puede
  anular.
- **Notas con huella de archivos:** al registrar una nota se pueden indicar archivos
  del NAS (factura, cotización, acta escaneada, foto). Se guarda la huella de cada uno.
  Después, «Comprobar archivos» indica si cada archivo sigue igual, cambió o ya no
  existe.

## El ancla: por qué hace falta

Quien controla el servidor podría borrar la base y **recalcular la cadena entera**,
porque las huellas no tienen secretos. La defensa es guardar la huella final **fuera
del servidor**:

- **Automático:** una vez al día LabNAS verifica la cadena. Si creció desde el último
  ancla, envía la huella final a los administradores por Telegram. Una cadena
  reescrita ya no podría terminar en las huellas que quedaron en esos chats.
- **Manual:** el botón «Anclar» hace lo mismo y muestra un texto para copiar en un
  correo (por ejemplo, a Finanzas al cerrar cada mes) o en un acta.
- Si la verificación diaria encuentra la cadena rota, o detecta que el último registro
  anclado ya no está, avisa a los administradores por Telegram y en la web, y deja
  constancia en la auditoría.

> Recomendación: al cierre de cada mes, anclar y enviar ese texto por correo
> institucional a una persona fuera del laboratorio. El correo queda en servidores de
> la universidad, fuera del alcance del laboratorio.

## Verificar sin LabNAS

«Exportar» descarga un archivo `.jsonl` con un registro por línea. Cualquiera puede
verificarlo con este script de Python, sin instalar nada:

```python
#!/usr/bin/env python3
"""Verifica una bitácora exportada de LabNAS: python3 verificar.py bitacora.jsonl"""
import hashlib, json, sys

prev, n = "0" * 64, 0
for line in open(sys.argv[1], encoding="utf-8"):
    e = json.loads(line)
    n += 1
    pre = json.dumps(
        [e["seq"], e["timestamp"], e["username"], e["kind"], e["subject"],
         e["annuls"], e["data"], e["prev_hash"]],
        ensure_ascii=False, separators=(",", ":"),
    )
    if e["seq"] != n:
        sys.exit(f"Falta el registro {n}")
    if e["prev_hash"] != prev:
        sys.exit(f"Registro {n}: no apunta al anterior")
    if hashlib.sha256(pre.encode("utf-8")).hexdigest() != e["hash"]:
        sys.exit(f"Registro {n}: el contenido no corresponde a su huella")
    prev = e["hash"]
print(f"OK: {n} registros. Huella final: {prev}")
```

Compare la huella final con la del último ancla (correo o Telegram). Si es la misma, o
si el registro anclado aparece en el archivo con esa huella, nada anterior al ancla fue
alterado.

### Receta exacta de la huella

```
hash = sha256_hex( JSON compacto de [seq, timestamp, username, kind, subject, annuls, data, prev_hash] )
```

- JSON compacto: sin espacios y sin escapar caracteres no ASCII (`ñ` va tal cual).
- `data` es un texto (el JSON del contenido tal como se guardó), no un objeto.
- `annuls` es `null` o el número del registro anulado.
- El primer registro usa `prev_hash` = 64 ceros.

## Para quien programa

- `crate::ledger::append(conn, usuario, tipo, asunto, anula, &json)` agrega un registro
  (transacción `BEGIN IMMEDIATE`, sin carreras). Los módulos nuevos usan tipos propios
  (por ejemplo `compra.solicitud`, `compra.aprobacion`).
- Tabla `ledger_entries` (migración 16): `prev_hash` y `hash` son únicos (la cadena no
  se puede bifurcar), `annuls` es único (no hay doble anulación) y hay triggers que
  impiden `UPDATE` y `DELETE`.
- No cambiar `compute_hash`: el test `receta_de_la_huella_es_estable` fija la receta con
  un vector calculado en Python.
- API (solo admin): `GET /api/ledger`, `GET /api/ledger/verify`,
  `GET /api/ledger/export`, `POST /api/ledger/notes`, `POST /api/ledger/{seq}/annul`,
  `GET /api/ledger/{seq}/files`, `POST /api/ledger/anchor`.

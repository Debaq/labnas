# Pantalla del laboratorio

Una TV o tablet en el laboratorio puede mostrar, en grande y sin iniciar sesión:

- **Máquinas**: libre, en uso (por quién y cuánto le queda), bloqueada o fuera de servicio,
  y la próxima reserva.
- **Impresoras 3D**: avance, archivo, tiempo restante y temperaturas.
- **Reservas de hoy**: lo que queda del día en todas las máquinas.
- **Sensores**: última lectura; en rojo lo que está fuera del umbral de una alerta.
- **Plazos cercanos**: los de las próximas dos semanas y los vencidos.
- **Avisos**: fallas abiertas, mantenciones vencidas y sensores fuera de rango.

Se actualiza sola cada 20 segundos y pide al navegador que no apague la pantalla.

## Cómo se crea

**Configuración > Laboratorio > Pantallas del laboratorio → Nueva pantalla.** Se le pone un
nombre (por ejemplo «TV de la entrada»), se eligen las secciones y se copia su enlace
(`/pantalla/<código>`) para abrirlo en el navegador de la TV o tablet, en pantalla completa.

- El enlace **solo sirve para mirar**: no da acceso a nada más de LabNAS.
- Cada pantalla tiene su enlace. Si se pierde o cambia la tablet, se **revoca** y se crea
  otra.
- La lista muestra cuándo pidió datos cada pantalla por última vez.
- Una sección cuyo módulo está apagado no aparece.
- Las impresoras 3D se muestran según lo que vio el monitor de impresoras en los últimos
  dos minutos (no se consultan en cada recarga).

Crear, editar y revocar pantallas es solo del administrador y queda en la auditoría.

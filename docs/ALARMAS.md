# Alarmas críticas con escalamiento

Para un freezer, una incubadora o una sala que no puede quedar fuera de rango sin que nadie
se entere. Una alerta de sensor normal avisa a todos los chats y se calla un rato
(cooldown). Una alerta **crítica** en cambio abre un **incidente** y busca a alguien que se
haga cargo.

## Cómo se configura

En **Sensores**, al crear una alerta (botón **Alerta** del sensor), marcar **Alarma crítica**
y armar la **lista de personas en orden**, más los minutos de espera antes de pasar a la
siguiente (10 por defecto).

## Qué pasa

1. La lectura sale de rango: se abre el incidente y se avisa por Telegram a la **primera
   persona** de la lista (y en la web). Sin lista, se avisa directo a quienes gestionan
   Sensores.
2. Esa persona responde **/visto** en Telegram o toca **Lo vi, lo atiendo** en Sensores. Los
   demás avisados reciben «X lo está atendiendo».
3. Si nadie confirma en N minutos, se avisa a la **siguiente**; al terminar la lista, a todos
   los que **gestionan Sensores**. Ahí se deja de escalar.
4. Cuando la lectura vuelve a rango el incidente se cierra y se avisa a quienes ya sabían,
   con cuánto duró y quién lo atendió (o que nadie lo confirmó).

Mientras el incidente está abierto, más lecturas fuera de rango no abren otro: se anota el
peor valor. Arriba de la página de Sensores se ven las alarmas abiertas y, plegado, el
historial. Todo queda en la auditoría.

Los avisos llegan a los chats de Telegram **vinculados a la cuenta web** de cada persona
(Configuración > Notificaciones). Alguien de la lista sin chat vinculado solo ve el aviso en
la web: conviene que todos lo vinculen.

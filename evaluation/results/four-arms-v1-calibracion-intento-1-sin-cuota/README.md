# Calibración 9b, intento 1: anulado por falta de cuota

Fecha: 2026-11-18. Las 16 corridas del baseline terminaron con
`execution_failed` en unos 2 s, sin uso de tokens: el agente era entonces GPT
vía Codex CLI, y el proveedor respondió `turn.failed` por límite de uso de la
suscripción de ChatGPT antes de que el modelo
contestara. Ninguna fila contiene una respuesta del modelo.

El runner de entonces contaba ese fallo como fallo técnico de cada fila y, con el
destino fijo, consumía la calibración. Se corrigió: un fallo del proveedor
(`provider_usage_limit`, `provider_turn_failed`) es ahora un error de entorno
que aborta sin escribir la fila (`fallo_del_proveedor` en `run_four_arms.py`).
La corrección cambió el plan congelado, así que este intento no puede
reanudarse: se conserva aquí sin cambios como registro y no entra en ningún
análisis. Después se decidió usar Claude como agente; la calibración se repetirá desde
cero, con Claude, en `four-arms-v1-calibracion/`.

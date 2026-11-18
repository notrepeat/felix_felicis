# 07 — Evidencia de utilidad para agentes

Afirmación: A11 («mejora a un agente», que declaramos no demostrada).
[Volver al maestro](00-maestro.md).

## Estado

- Piloto pareado de fase 07: 12/12 éxitos en ambos brazos y **cero llamadas a
  Felix**; resultado neutral, gate de beneficio no superado
  ([informe](../../evaluation/paired-report.md)).
- `evaluation/trajectory.py` deriva adopción, primer delta, rechazos,
  reparación, estado final y lugar del error.
- `evaluation/mcp_proxy_v2.py` agrega latencia y bytes por llamada. Una revisión
  independiente encontró defectos que se reprodujeron con un servidor falso.
  **Actualización 2026-11-17: corregidos.** Cada uno tiene un test en
  `evaluation/test_mcp_proxy_v2.py` que falló contra el código anterior (por la
  razón indicada) y pasa con la corrección:

| Defecto | Resultado observado antes | Test (rojo → verde) |
|---|---|---|
| A. `response_bytes` medía la línea del hijo, no la que recibe el agente | ledger 104 bytes, agente recibió 93; en el test, 131 frente a 133 | `test_response_bytes_measure_the_line_the_agent_receives` |
| B. La latencia absorbía el vaciado de respuestas anteriores hacia un agente lento | 986 ms para una respuesta inmediata | `test_slow_agent_does_not_inflate_latency_of_next_call` |
| B. `started` se tomaba antes de escribir la petición al hijo | 1010 ms para una petición grande a un hijo que tarda en leer | `test_latency_starts_after_request_is_written` |
| F. Llamadas reenviadas sin respuesta al cerrar no dejaban entrada | 0 entradas | `test_unanswered_forwarded_call_is_ledgered_on_shutdown` |
| C. Id repetido en vuelo sobrescribía `pending` y emparejaba mal (heredado de v1) | respuesta de la primera atribuida a la segunda | `test_duplicate_in_flight_id_is_rejected_not_mismatched` |
| D. `tools/call` sin id emparejado con una notificación del servidor (v1) | la notificación quedaba como respuesta | `test_call_without_id_is_not_paired_with_server_notification` |
| E. Petición servidor→cliente con el mismo id tomada como respuesta (v1) | `roots/list` registrada como respuesta | `test_server_request_with_same_id_is_not_taken_as_response` |
| G. Línea no JSON o lote en la salida del hijo mataba el lector (v1) | el agente no recibía la respuesta; también una línea no JSON del agente mataba el proxy | `test_non_json_and_batch_lines_do_not_kill_the_bridge` |
| H. Hijo que muere con una llamada en vuelo: el agente esperaba indefinidamente (hallado en la revisión de la corrección) | sin respuesta mientras el agente mantiene stdin abierto | `test_child_exit_with_call_in_flight_answers_the_agent` |
| I. Sustituto suelto (`\ud800`) o anidación profunda en una línea mataban el lector o el registro del ledger (revisión de la corrección) | `UnicodeEncodeError` / `RecursionError` en el hilo lector | `test_unencodable_or_deep_lines_do_not_kill_the_bridge` |

Límites que quedan, documentados en el docstring y sin test que los cubra:
al cerrar, el hijo tiene 2 s para responder antes de matarlo (lo pendiente queda
`unanswered`); si el agente deja de leer sin cerrar su salida, el proxy espera
indefinidamente a vaciar la cola hacia él; la latencia incluye el manejo propio
del proxy de líneas anteriores del hijo (parseo y ledger, normalmente <1 ms).

Definición vigente de la latencia: desde que la petición quedó escrita y
vaciada en la entrada del hijo hasta que el proxy leyó la línea completa de
respuesta; la entrega al agente va por un hilo escritor con cola y no cuenta.
El contrato completo está en el docstring del proxy. El ledger conserva el
esquema `felix-mcp-ledger-v2` con marcas nuevas (`rejected`, `no_id`,
`unanswered`): no había datos reales registrados con v2.

Sigue pendiente: el proxy v2 no está conectado a `run_paired.py` (E3, con
compuerta), y no hay mediciones reales con él.

## Qué falta para poder afirmar utilidad

| # | Brecha | Tamaño | Compuerta |
|---|---|---|---|
| E1 | ~~Corregir proxy v2 (bytes, latencia, llamadas en vuelo, robustez de ids) con tests que fallen ante esos defectos~~ **Hecho** (tabla de arriba) | S | No |
| E2 | Pruebas de `evaluation/` y de `tools/` en CI. **Configurado** en `.github/workflows/ci.yml` para Linux y Windows; en Linux pasan localmente. En Windows aún no se ha ejecutado: dos tests de `run_paired.bounded_capture` se saltan allí porque usan `os.killpg` (POSIX, archivo congelado), y `.gitattributes` fija LF en los cinco archivos con hash del piloto 173 | S | No |
| E3 | **Preparado** (2026-11-18, [propuesta 09](../propuestas/09-estudio-cuatro-brazos.md), [protocolo](../../evaluation/four_arms_protocol.md); cubre E3–E6): runner `run_four_arms.py` con proxy v2, cuatro brazos, tareas nuevas, oráculo independiente y plan congelado; sin corridas reales. Protocolo v2 con runner que monte el proxy v2 | S | **Sí**: forma parte del estudio nuevo |
| E4 | Brazos «verificación obligatoria» y «control de revisión sin veredicto» | M | **Sí** |
| E5 | Tareas nuevas con margen de mejora (dependencias largas, contradicciones reales del baseline), oracle independiente, criterio de detención prerregistrado | M | **Sí** |
| E6 | Medir también la seguridad del flujo (cambios inválidos aceptados, escrituras fuera de Felix), no solo el acierto del modelo | M | **Sí**, junto con E4 |

La conclusión de fase 07 sigue vigente: no avanzar a AGM ni recuperación
avanzada con esta evidencia. E1 y E2 son preparación que no cruza compuertas.

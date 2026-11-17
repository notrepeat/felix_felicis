# Validación empírica: fase 07

El piloto real no demostró mejora con Felix: el modelo resolvió correctamente
12/12 ejecuciones sin Felix y 12/12 con Felix disponible, sin usar las herramientas
en este último brazo. La disponibilidad real se comprobó aparte con una llamada
MCP ejecutada por el modelo. Esto mide un caso con éxito máximo y sin asistencia
efectiva; no demuestra inutilidad general ni capacidad de reparación.

El [informe pareado](../evaluation/paired-report.md) conserva prompts, respuestas,
uso reportado, tiempos y límites. Fueron seis tareas sintéticas, dos repeticiones
y el mismo modelo/prompt/presupuesto por par. El oracle independiente exige
cumplir el objetivo y preservar hechos ajenos, además de validez estructural.
No hay facturación observada: el costo monetario permanece desconocido.

Separadamente, el [corpus formal y costo del motor](../evaluation/formal-and-performance.md)
cubre 46 etiquetas exactas, las 14 variantes de fórmula y tres clases de conflicto,
sin falsos admit. La matriz de 70 muestras conserva cinco errores por agotamiento
del presupuesto con tres variables; los otros 65 casos admiten. No se oculta ese
límite cambiando la cuota después de medir.

La decisión del plan se mantiene: **no avanzar a AGM ni recuperación avanzada**
con esta evidencia; primero debe revisarse la hipótesis y explicar la falta de
beneficio en el loop mínimo. No se ejecutó la integración privada de fase 08.
La fase documenta mediciones y límites, sin una promesa de superioridad del agente.

Cierre verificable: informe `3f2cfe6`, CI `44152198690` aprobada en Linux y
Windows para Rust/corpus. El piloto real, los benchmarks y 19 pruebas Python
se ejecutaron localmente en Linux y se revisaron por separado.
[Checklist de fase 07](plan/07-validacion-empirica.md).
